//! Narrow-trait `Dropdown` (Phase 5p — first popover widget through `Paint::popover` /
//! `draw_popover`, the 5o surface). Detached-label control on the Slider convention (no rect
//! inflation; the label eats into the assigned rect), `Control::control_label`'s +4px inset via
//! `Layout::detached_label_inset`, side-label inset computed from the synced label.
//!
//! Parity notes (all legacy-faithful, verified against the pre-migration impl):
//! - `parent_snapshot` is the data form of the legacy public, direct-write-only `parent`
//!   pointer: legacy `set_parent` never wrote it (the WidgetHost default only touched the tree —
//!   Ramp's dummy-ctx `set_parent` calls were silently discarded), so the Ramp popover clamp
//!   and the fade-blend parent color activate only for callers that assign the field, exactly
//!   as before — no production writer exists. The backplate-concentric corner walk it once
//!   anchored is gone outright (replaced by the app-owned `corner_frame`, Phase 6s).
//! - The row-rect hit expansion (`base.row_x/row_w`) is dropped, consistent with every other
//!   migrated control: `Input::hit` tests the widget rect plus the open popover.
//! - `Layout::intrinsic_measure_width` (new hook) preserves the `auto_width` measure behavior
//!   (cce-system-interface sizes its page dropdown from `WidgetHost::measure`).

use crate::colors;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::model::{Adapted, EventCtx, Input, Layout, Paint};
use crate::widget::{
    WidgetHost, ElementState, Event, Key, MouseButton, NamedKey,
};

/// Read-data stand-in for the legacy direct-write `parent` pointer (6bd — no stored widget
/// pointers): the popover clamp and bg fade-blend read the host's rect/kind/color ctx-less at
/// paint time. Callers that want the Ramp clamp assign it directly, same activation model as
/// the old field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParentSnapshot {
    pub rect: (f32, f32, f32, f32),
    pub is_ramp: bool,
    pub color: [f32; 4],
}

/// Monospace detection — the paint pass lays characters out on a fixed cell in a monospace font
/// and on measured per-character advances otherwise, so the sizing pass must branch the same way.
fn is_monospace_font(font_family: &str, font_size: f32) -> bool {
    let w_i10 = crate::widget::display::measure_text_width("iiiiiiiiii", font_family, font_size);
    let w_m10 = crate::widget::display::measure_text_width("mmmmmmmmmm", font_family, font_size);
    (w_i10 - w_m10).abs() < 5.0
}

/// The fixed per-character cell of a monospace font, taken as the slope between a 10- and a
/// 20-`m` run so any constant side bearing in the measurement cancels out.
fn monospace_cell_width(font_family: &str, font_size: f32) -> f32 {
    let w_m10 = crate::widget::display::measure_text_width("mmmmmmmmmm", font_family, font_size);
    let w_m20 = crate::widget::display::measure_text_width("mmmmmmmmmmmmmmmmmmmm", font_family, font_size);
    ((w_m20 - w_m10) / 10.0).max(1.0)
}

/// The advance width `paint_text` will actually lay `text` out to.
///
/// This is the single source of truth shared by the sizing pass (`content_width` /
/// `display_width`) and the paint pass. It is deliberately NOT a plain `measure_text_width`: the
/// paint pass advances on the monospace cell or on the M-dummy trick, and either can exceed the
/// raw ink measure. Sizing a dropdown from the ink measure therefore left the trigger a few px too
/// narrow and tripped its own right-edge fade — badly under a monospace UI font like the default
/// Berkeley Mono. Keep this in step with `paint_text`.
fn text_advance(text: &str, font_family: &str, font_size: f32) -> f32 {
    let n = text.chars().count();
    if n == 0 {
        return 0.0;
    }
    if is_monospace_font(font_family, font_size) {
        n as f32 * monospace_cell_width(font_family, font_size)
    } else {
        let w_dummy = crate::widget::display::measure_text_width("M", font_family, font_size);
        let measure_str = format!("{}M", text);
        (crate::widget::display::measure_text_width(&measure_str, font_family, font_size) - w_dummy).max(0.0)
    }
}

/// Side-layout label inset — the legacy `WidgetHost::label_x_offset` default for non-exempt
/// widgets (Dropdown was never in the exempt list).
fn side_offset(label: &Option<String>) -> f32 {
    if crate::layout::control_label_layout() == "side" && label.is_some() {
        90.0
    } else {
        0.0
    }
}

#[derive(Debug, Clone)]
pub struct Dropdown {
    pub options: Vec<String>,
    pub selected: usize,
    pub open: bool,
    pub(crate) hovered_item: Option<usize>,
    just_changed: bool,
    /// Host read-data, written ONLY by direct assignment (the Ramp-clamp unit test; no
    /// production writer). Read by the Ramp popover clamp and the fade-blend parent color,
    /// like the legacy `parent` pointer it replaces.
    pub parent_snapshot: Option<ParentSnapshot>,
    pub font_family: String,
    pub custom_display_text: Option<String>,
    pub open_upward: Option<bool>,
    pub auto_width: bool,
    /// Synced copy of the control label ([`Paint::sync_label`]) — drives the side/detached
    /// offsets the base label geometry imposes on the widget's own geometry.
    label: Option<String>,
    /// Own hover flag, maintained from `MouseEnter`/`MouseLeave` (the adapter's hover
    /// bookkeeping hit-tests through [`Input::hit`], which includes the open popover — matching
    /// the legacy `on_cursor_moved` + popover-aware `hit_test` pair).
    hovered: bool,
    /// App-owned concentric frame (Phase 6s): `(rect, radius, corners)` of the rounded plate
    /// the dropdown sits in. When set, the corner adjustment uses it INSTEAD of walking for a
    /// `Backplate` ancestor — the hook that keeps the adjustment after an app dissolves its
    /// root Backplate (the walk finds nothing once the widget is parentless).
    corner_frame: Option<((f32, f32, f32, f32), f32, (bool, bool, bool, bool))>,
    /// Raised style: the closed control's background is an SDF-lit `Bevel`
    /// plate (fill + rolled lit edge) instead of a flat fill + border stroke.
    raised: bool,
    /// Expand/contract animation (the status-interface module-menu feel).
    /// WALL-CLOCK, not dt-stepped: progress runs from `anim_from` at
    /// `anim_start` toward 1 (or 0 while `closing`) over [`Self::ANIM_S`], read
    /// at draw time — so an app that never ticks its UiContext can never strand
    /// the popover mid-size; ticks only drive redraws and settle a landed
    /// close. `closing` keeps `open` (and the shrinking popover) alive while
    /// everything interactive gates on `!closing`.
    anim_from: f32,
    anim_start: Option<std::time::Instant>,
    closing: bool,
    /// Per-frame SNAPSHOT of the wall-clock progress, refreshed in `tick`
    /// (before each render) and on every routed event. All geometry readers —
    /// popover_rect at registration, draw_popover, the engine's occlusion
    /// clamp — use this one value, so the animated rect is stable within a
    /// frame: the clamp's exact-match overlay-text exemption compares text
    /// bounds against popover_rect evaluated later in the same pass, and a
    /// live clock read there would never match.
    anim_snap: f32,
}

impl Dropdown {
    pub fn new(options: Vec<String>, selected: usize) -> Adapted<Dropdown> {
        Adapted::new(Dropdown {
            options,
            selected,
            open: false,
            hovered_item: None,
            just_changed: false,
            parent_snapshot: None,
            font_family: "sans-serif".to_string(),
            custom_display_text: None,
            open_upward: None,
            auto_width: false,
            label: None,
            hovered: false,
            corner_frame: None,
            raised: crate::layout::control_relief(),
            anim_from: 0.0,
            anim_start: None,
            closing: false,
            anim_snap: 0.0,
        })
    }

    /// Set (or clear) the app-owned concentric frame — see the `corner_frame` field docs.
    pub fn set_corner_frame(&mut self, frame: Option<((f32, f32, f32, f32), f32, (bool, bool, bool, bool))>) {
        self.corner_frame = frame;
    }

    pub fn take_change(&mut self) -> bool {
        let changed = self.just_changed;
        self.just_changed = false;
        changed
    }

    /// Horizontal inset added to a measured label to get a dropdown width the label fits inside
    /// without tripping `paint_text`'s right-edge fade: an 8px left pad plus the 28px right
    /// reservation (`right_limit = w - 28`, room for the 10px gap and the ▼ arrow) = 36px, plus a
    /// 2px cushion for the small gap between the ink-`measure_text_width` used here and the M-dummy
    /// advance the paint pass measures with. Shared by `content_width` (widest option) and
    /// `display_width` (collapsed display text) so the two can't drift.
    const LABEL_INSET: f32 = 38.0;

    pub fn content_width(&self) -> f32 {
        let font_setting = crate::layout::control_label_font_detached();
        let (font_family, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);
        let mut max_w = 0.0f32;
        for opt in &self.options {
            let opt_w = text_advance(opt, &font_family, font_size) + Self::LABEL_INSET;
            if opt_w > max_w {
                max_w = opt_w;
            }
        }
        max_w
    }

    /// The text shown on the collapsed trigger — the fixed `custom_display_text` (menu-button
    /// mode) if set, otherwise the selected option. Mirrors the selection logic in `paint_text`.
    fn display_text(&self) -> String {
        if let Some(ref custom_text) = self.custom_display_text {
            custom_text.clone()
        } else {
            self.options.get(self.selected).cloned().unwrap_or_default()
        }
    }

    /// Trigger width sized to the collapsed display text rather than the widest option (via
    /// `content_width`). Used by menu-button dropdowns whose label is fixed, so "File"/"Edit" don't
    /// stretch to their longest menu entry. Shares `LABEL_INSET` so the label fits without fading.
    fn display_width(&self) -> f32 {
        let font_setting = crate::layout::control_label_font_detached();
        let (font_family, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);
        text_advance(&self.display_text(), &font_family, font_size) + Self::LABEL_INSET
    }

    /// The detached-label strip height above the content rect — a replica of
    /// `Widget::label_offset` over the synced label (zero in side layout or unlabeled).
    fn label_top(&self) -> f32 {
        if crate::layout::control_label_layout() == "side" {
            return 0.0;
        }
        if self.label.is_some() {
            let (_, font_size) = crate::layout::control_label_font_detached_parsed();
            font_size + crate::layout::control_label_margin()
        } else {
            0.0
        }
    }

    /// Expansion/contraction duration — the status-interface module-menu pace.
    const ANIM_S: f32 = 0.14;

    /// LIVE animation progress in [0, 1], wall-clock from the last transition.
    /// 1 = fully open, 0 = fully contracted. Geometry never reads this
    /// directly — it reads the per-frame `anim_snap` (see the field docs).
    fn anim_progress_now(&self) -> f32 {
        let Some(start) = self.anim_start else {
            return if self.open && !self.closing { 1.0 } else { 0.0 };
        };
        let el = start.elapsed().as_secs_f32() / Self::ANIM_S;
        if self.closing {
            (self.anim_from - el).clamp(0.0, 1.0)
        } else {
            (self.anim_from + el).clamp(0.0, 1.0)
        }
    }

    fn begin_open(&mut self) {
        self.anim_from = self.anim_progress_now();
        self.anim_start = Some(std::time::Instant::now());
        self.open = true;
        self.closing = false;
        self.anim_snap = self.anim_from;
    }

    fn begin_close(&mut self) {
        if !self.open || self.closing {
            return;
        }
        self.anim_from = self.anim_progress_now();
        self.anim_start = Some(std::time::Instant::now());
        self.closing = true;
        self.anim_snap = self.anim_from;
    }

    /// Fold finished animations back into settled state and refresh the
    /// per-frame progress snapshot (draw paths are `&self`, so this runs from
    /// the mutation entry points: `tick` and `on_event`). Also heals an
    /// externally forced `open = false` (a direct field write skips the
    /// animation; reset so the next open still animates).
    fn settle_anim(&mut self) {
        if self.closing {
            if self.anim_progress_now() <= 0.0 {
                self.closing = false;
                self.open = false;
                self.anim_start = None;
                self.hovered_item = None;
            }
        } else if self.open {
            if self.anim_progress_now() >= 1.0 {
                self.anim_start = None;
            }
        } else {
            self.anim_start = None;
        }
        self.anim_snap = self.anim_progress_now();
    }

    /// Land an in-flight open or close instantly (tests can't wait out the
    /// wall clock).
    #[cfg(test)]
    fn land_anim_for_test(&mut self) {
        self.anim_start = Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
        self.settle_anim();
    }

    fn popover_width(&self, content: Rect) -> f32 {
        content.width.max(self.content_width())
    }

    /// The ONE continuous surface drawn while open: the trigger band unioned
    /// with the revealed menu area — the status-interface treatment, where the
    /// module box literally grows into its menu instead of spawning a detached
    /// popover plate.
    fn unified_geom_drawn(&self, content: Rect) -> (f32, f32, f32, f32) {
        let (ax, ay, aw, ah) = self.popover_geom_drawn(content);
        let label_x = side_offset(&self.label);
        let (tx, ty) = (content.x + label_x, content.y);
        let (tw, th) = ((content.width - label_x).max(0.0), content.height);
        let x0 = tx.min(ax);
        let y0 = ty.min(ay);
        let x1 = (tx + tw).max(ax + aw);
        let y1 = (ty + th).max(ay + ah);
        (x0, y0, x1 - x0, y1 - y0)
    }

    /// The menu box revealed this frame: the full geometry with the height
    /// revealed — and the width grown out of the trigger — by the eased
    /// progress (cubic-out, the status-interface module-menu curve). Rows keep
    /// their final positions and slide into view under the traveling edge; an
    /// upward popover anchors its bottom edge to the trigger instead.
    fn popover_geom_drawn(&self, content: Rect) -> (f32, f32, f32, f32) {
        let (rx, ry, rw, rh) = self.popover_geom(content);
        let a = self.anim_snap;
        if a >= 1.0 {
            return (rx, ry, rw, rh);
        }
        let t = 1.0 - (1.0 - a) * (1.0 - a) * (1.0 - a);
        let base_y = content.y - self.label_top();
        let open_upward = self.open_upward.unwrap_or(base_y > 400.0);
        let w0 = content.width.min(rw);
        let aw = w0 + (rw - w0) * t;
        let ah = rh * t;
        let ay = if open_upward { ry + rh - ah } else { ry };
        (rx, ay, aw, ah)
    }

    /// Popover geometry against the laid-out content rect — the legacy `get_popover_geom`,
    /// with the base-rect reads rewritten in content-rect terms (`base.y + base.h` ⇒
    /// `content.y + content.height`, `base.y + label_offset` ⇒ `content.y`).
    pub fn popover_geom(&self, content: Rect) -> (f32, f32, f32, f32) {
        let rw = self.popover_width(content);
        let rh = self.options.len() as f32 * 24.0;

        let base_y = content.y - self.label_top();
        let open_upward = self.open_upward.unwrap_or(base_y > 400.0);
        let label_x = side_offset(&self.label);

        let mut rx = content.x + label_x;
        let mut ry = if open_upward {
            content.y - rh
        } else {
            content.y + content.height
        };

        let is_ramp = self.parent_snapshot.map_or(false, |s| s.is_ramp);

        if is_ramp {
            if let Some(snap) = self.parent_snapshot {
                let (px, py, pw_parent, ph_parent) = snap.rect;
                if pw_parent > 0.0 && ph_parent > 0.0 {
                    let dy_down = content.y + content.height;
                    let dy_up = content.y - rh;

                    if self.open_upward.is_none() {
                        if dy_down + rh > py + ph_parent && dy_up >= py {
                            ry = dy_up;
                        } else if dy_up < py && dy_down + rh <= py + ph_parent {
                            ry = dy_down;
                        }
                    }

                    // Clamp X to parent borders
                    if rx < px {
                        rx = px;
                    }
                    if rx + rw > px + pw_parent {
                        rx = px + pw_parent - rw;
                    }

                    // Clamp Y to parent borders
                    if ry < py {
                        ry = py;
                    }
                    if ry + rh > py + ph_parent {
                        ry = py + ph_parent - rh;
                    }
                }
            }
        }

        (rx, ry, rw, rh)
    }

    fn border_color(&self) -> [f32; 4] {
        if self.open && !self.closing {
            [0.30, 0.50, 0.32, 1.0]
        } else if self.hovered {
            let bc = colors::dropdown_border_color();
            [(bc[0] + 0.15).min(1.0), (bc[1] + 0.15).min(1.0), (bc[2] + 0.15).min(1.0), bc[3]]
        } else {
            colors::dropdown_border_color()
        }
    }

    /// Emit the border + background geometry — the legacy `all_rounded_quads` body (rounded,
    /// with the backplate-concentric corner adjustment) or `extra_quads` (plain) depending on
    /// the configured radius, byte-for-byte on the same content rect.
    fn paint_background(&self, content: Rect, ctx: &mut PaintCtx) {
        let label_x = side_offset(&self.label);
        let x = content.x + label_x;
        let w = content.width - label_x;
        let y = content.y;
        let visual_h = content.height;

        let raw_bg = colors::dropdown_background_color();
        let mut bg_color = raw_bg;
        bg_color[3] = 1.0; // Force opaque background to prevent subpixel blending artifacts
        let border_color = self.border_color();

        let radius = crate::layout::dropdown_corner_radius();
        // Raised style: one lit Bevel plate owns fill and edge (the concentric
        // corner_frame adjustment keeps the legacy path — it exists to nest
        // flat outlines, which a rolled edge replaces). A transparent
        // configured fill degrades to a Boss: edges only, plate as the face —
        // judged on the RAW alpha, before the opacity force above.
        if self.raised {
            let depth = crate::layout::bevel_width().min(visual_h * 0.2);
            // Concentric corner_frame adjustment applies to the relief too: a
            // corner nested at equal gaps into the frame follows its curve.
            // The window corner is span-widened (corner_span_factor, diagonal
            // curvature = pr), so its parallel curve at inset g has diagonal
            // curvature pr - g — which as a NOMINAL widget-scale squircle
            // radius is factor * (pr - g). Exactly pr - g for circular
            // corners (factor 1).
            let mut r4 = [radius; 4];
            if let Some(((px, py, pw, ph), pr, (pr1, pr2, pr3, pr4))) = self.corner_frame {
                let cf = crate::layout::corner_span_factor();
                let g_left = x - px;
                let g_top = y - py;
                let g_right = (px + pw) - (x + w);
                let g_bottom = (py + ph) - (y + visual_h);
                if pr1 && (g_left - g_top).abs() < 1.0 && g_left >= 0.0 {
                    r4[0] = (pr - g_left).max(0.0) * cf;
                }
                if pr2 && (g_right - g_top).abs() < 1.0 && g_right >= 0.0 {
                    r4[1] = (pr - g_right).max(0.0) * cf;
                }
                if pr3 && (g_right - g_bottom).abs() < 1.0 && g_right >= 0.0 {
                    r4[2] = (pr - g_right).max(0.0) * cf;
                }
                if pr4 && (g_left - g_bottom).abs() < 1.0 && g_left >= 0.0 {
                    r4[3] = (pr - g_left).max(0.0) * cf;
                }
            }
            // Flush inset plate: groove ring down, beveled lip back up, face
            // level with the surface (transparent raw fill = edges only).
            let face = if raw_bg[3] > 0.001 { bg_color } else { [0.0; 4] };
            let strip = self.label_top();
            if strip > 0.0 {
                // Labeled: the label sits in a CARVE-OUT tab, the section-
                // title idiom — a flat recessed well hugging the label run,
                // its bottom open into the trigger's groove ring below (the
                // tab's walls: top, right, left). Right of the tab the ring
                // keeps its normal top wall, starting at the tab's throat.
                let g = depth * 0.5;
                let orad = (r4[0] + g, r4[1] + g, r4[2] + g, r4[3] + g);
                let (outer_x, outer_r) = (x - g, x + w + g);
                let (tab_top, ring_top) = (y - strip - g, y - g);
                // The tab hugs the label run (drawn at x + inset): text width
                // plus the inset each side, kept inside the trigger's span.
                let (fam, fsize) = crate::layout::control_label_font_detached_parsed();
                let text_w = self
                    .label
                    .as_deref()
                    .map(|l| crate::widget::display::measure_text_width(l, &fam, fsize))
                    .unwrap_or(0.0);
                let inset = 4.0; // Layout::detached_label_inset — the label's x offset
                let tab_w = (text_w + 2.0 * inset + 2.0 * g)
                    .max(2.0 * orad.0 + 8.0)
                    .min(outer_r - outer_x);
                let tab_r = outer_x + tab_w;

                // The tab: bottom open into the ring, pieces extended `depth`
                // past their interior seams so the tessellator's host fades
                // crossfade there instead of notching the walls. With the
                // fillet, the tab's right wall must END at the fillet's
                // vertical tangent (crossfading out under the arc) or its
                // straight run ghosts through the curve — the tab piece stops
                // there and a left-only bridge carries the left wall across
                // the fillet span down to the ring's own fade-in.
                let fr = 6.0_f32.min(strip * 0.5);
                let filleted = outer_r - tab_r > fr + 4.0;
                let tab_bottom = if filleted { ring_top - fr } else { ring_top };
                ctx.recess_edges(
                    Rect { x: outer_x, y: tab_top, width: tab_w, height: tab_bottom - tab_top + depth },
                    (orad.0, orad.1.min(strip * 0.5), 0.0, 0.0),
                    depth,
                    (true, true, false, true),
                );
                if filleted {
                    ctx.recess_edges(
                        Rect { x: outer_x, y: ring_top - fr, width: tab_w, height: fr + depth },
                        (0.0, 0.0, 0.0, 0.0),
                        depth,
                        (false, false, false, true),
                    );
                }
                // The ring proper: right + bottom + left walls, one prim so
                // its corners blend internally.
                ctx.recess_edges(
                    Rect { x: outer_x, y: ring_top, width: w + 2.0 * g, height: visual_h + 2.0 * g },
                    (0.0, 0.0, orad.2, orad.3),
                    depth,
                    (false, true, true, true),
                );
                // Ring top wall, right of the tab. The concave fillet rounds
                // the throat; the straight run starts a fillet radius past it
                // (extended `depth` left so its fade-in lands under the
                // fillet's hard tangent cut instead of leaving a gap).
                if filleted {
                    ctx.concave_fillet(
                        tab_r + fr,
                        ring_top - fr,
                        fr,
                        depth,
                        std::f32::consts::FRAC_PI_2,
                        false,
                    );
                    ctx.recess_edges(
                        Rect {
                            x: tab_r + fr - depth,
                            y: ring_top,
                            width: outer_r - tab_r - fr + depth,
                            height: visual_h + 2.0 * g,
                        },
                        (0.0, orad.1, 0.0, 0.0),
                        depth,
                        (true, false, false, false),
                    );
                } else if outer_r - tab_r > 0.5 {
                    ctx.recess_edges(
                        Rect { x: tab_r - depth, y: ring_top, width: outer_r - tab_r + depth, height: visual_h + 2.0 * g },
                        (0.0, orad.1, 0.0, 0.0),
                        depth,
                        (true, false, false, false),
                    );
                }
                let rect = Rect { x, y, width: w, height: visual_h };
                let rrad = (r4[0], r4[1], r4[2], r4[3]);
                if face[3] > 0.001 {
                    ctx.bevel(rect, rrad, face, depth);
                } else {
                    ctx.boss(rect, rrad, depth);
                }
                return;
            }
            ctx.inset_plate(
                Rect { x, y, width: w, height: visual_h },
                (r4[0], r4[1], r4[2], r4[3]),
                face,
                depth,
            );
            return;
        }
        if radius <= 0.0 {
            ctx.quad(Rect { x, y, width: w, height: visual_h }, border_color);
            ctx.quad(
                Rect { x: x + 1.0, y: y + 1.0, width: w - 2.0, height: visual_h - 2.0 },
                bg_color,
            );
            return;
        }

        let inner_radius = (radius - 1.0).max(0.0);
        let mut adjusted = false;
        let mut outer_radii = [radius; 4];
        let mut inner_radii = [inner_radius; 4];

        // Only an explicit corner_frame adjusts concentric corners now — the legacy fallback
        // walked ancestors for a backplate, which no longer exists.
        let frame = self.corner_frame;
        if let Some(((px, py, pw, ph), pr, (pr1, pr2, pr3, pr4))) = frame {
            let g_left = x - px;
            let g_top = y - py;
            let g_right = (px + pw) - (x + w);
            let g_bottom = (py + ph) - (y + visual_h);

            if pr1 && (g_left - g_top).abs() < 1.0 && g_left >= 0.0 {
                outer_radii[0] = (pr - g_left).max(0.0);
                inner_radii[0] = (outer_radii[0] - 1.0).max(0.0);
                adjusted = true;
            }
            if pr2 && (g_right - g_top).abs() < 1.0 && g_right >= 0.0 {
                outer_radii[1] = (pr - g_right).max(0.0);
                inner_radii[1] = (outer_radii[1] - 1.0).max(0.0);
                adjusted = true;
            }
            if pr3 && (g_right - g_bottom).abs() < 1.0 && g_right >= 0.0 {
                outer_radii[2] = (pr - g_right).max(0.0);
                inner_radii[2] = (outer_radii[2] - 1.0).max(0.0);
                adjusted = true;
            }
            if pr4 && (g_left - g_bottom).abs() < 1.0 && g_left >= 0.0 {
                outer_radii[3] = (pr - g_left).max(0.0);
                inner_radii[3] = (outer_radii[3] - 1.0).max(0.0);
                adjusted = true;
            }
        }

        if adjusted {
            for (qx, qy, qw, qh, qr, qc, corners) in crate::layout::partition_concentric_corners(
                x, y, w, visual_h, radius, outer_radii, border_color,
            ) {
                ctx.rounded_rect(Rect { x: qx, y: qy, width: qw, height: qh }, qr, corners, qc);
            }
            for (qx, qy, qw, qh, qr, qc, corners) in crate::layout::partition_concentric_corners(
                x + 1.0, y + 1.0, w - 2.0, visual_h - 2.0, inner_radius, inner_radii, bg_color,
            ) {
                ctx.rounded_rect(Rect { x: qx, y: qy, width: qw, height: qh }, qr, corners, qc);
            }
        } else {
            let corners = (true, true, true, true);
            ctx.rounded_rect(Rect { x, y, width: w, height: visual_h }, radius, corners, border_color);
            ctx.rounded_rect(
                Rect { x: x + 1.0, y: y + 1.0, width: w - 2.0, height: visual_h - 2.0 },
                inner_radius,
                corners,
                bg_color,
            );
        }
    }

    /// Emit the selected-text (per-character fade against the right edge) and the ▼ arrow —
    /// the legacy `text_labels` body minus the control label (the adapter's base-label
    /// machinery draws that, with the +4px `detached_label_inset`).
    fn paint_text(&self, content: Rect, ctx: &mut PaintCtx) {
        let selected_text = if let Some(ref custom_text) = self.custom_display_text {
            custom_text.clone()
        } else {
            self.options.get(self.selected).cloned().unwrap_or_default()
        };

        let (font_family, font_size) = crate::layout::control_label_font_detached_parsed();
        let label_x = side_offset(&self.label);
        let x = content.x + label_x;
        let w = content.width - label_x;
        let start_x = x + 8.0;
        let right_limit = x + w - 28.0; // 10px margin before the arrow
        let fade_start_x = (right_limit - 24.0).max(start_x); // Fade out over the last 24px
        let text_y = crate::layout::center_text_y(content.y, content.height, font_size);
        let tc = colors::dropdown_text_color();
        let default_color = [
            (colors::linear_to_srgb(tc[0]) * 255.0).round() as u8,
            (colors::linear_to_srgb(tc[1]) * 255.0).round() as u8,
            (colors::linear_to_srgb(tc[2]) * 255.0).round() as u8,
        ];
        let bg_color = colors::dropdown_background_color();
        let mut parent_color = colors::page_color();
        if let Some(snap) = self.parent_snapshot {
            parent_color = snap.color;
        }
        let alpha = 1.0; // The dropdown background is drawn fully opaque
        let bg_rgb = [
            ((parent_color[0] * (1.0 - alpha) + bg_color[0] * alpha) * 255.0).round().clamp(0.0, 255.0) as u8,
            ((parent_color[1] * (1.0 - alpha) + bg_color[1] * alpha) * 255.0).round().clamp(0.0, 255.0) as u8,
            ((parent_color[2] * (1.0 - alpha) + bg_color[2] * alpha) * 255.0).round().clamp(0.0, 255.0) as u8,
        ];

        let w_dummy = crate::widget::display::measure_text_width("M", &font_family, font_size);
        let chars: Vec<char> = selected_text.chars().collect();
        let n = chars.len();

        let is_monospace = is_monospace_font(&font_family, font_size);

        let cell_width = if is_monospace {
            monospace_cell_width(&font_family, font_size)
        } else {
            0.0
        };

        let mut char_offsets = Vec::with_capacity(n);
        if is_monospace {
            for i in 0..n {
                char_offsets.push(i as f32 * cell_width);
            }
        } else {
            if n > 0 {
                char_offsets.push(0.0f32);
            }
            let mut prefix = String::new();
            for i in 1..n {
                prefix.push(chars[i - 1]);
                let measure_str = format!("{}M", prefix);
                let w_prefix_dummy = crate::widget::display::measure_text_width(&measure_str, &font_family, font_size);
                let offset = (w_prefix_dummy - w_dummy).max(0.0);
                char_offsets.push(offset);
            }
        }

        let total_advance = text_advance(&selected_text, &font_family, font_size);

        // Draw and fade every character individually
        let mut prev_char_end = 0.0;
        for i in 0..n {
            let mut offset = char_offsets[i];
            if !is_monospace {
                if i > 0 {
                    offset = offset.max(prev_char_end + 1.0);
                }
            }
            let next_offset = if i < n - 1 { char_offsets[i + 1] } else { total_advance };
            let c_w = if is_monospace { cell_width } else { next_offset - offset };
            let cur_x = start_x + offset;

            if cur_x >= right_limit {
                break;
            }

            let char_mid_x = cur_x + c_w / 2.0;
            let mut skip_char = false;
            let text_end_x = start_x + total_advance;
            let color = if text_end_x > right_limit && char_mid_x > fade_start_x {
                let factor = ((char_mid_x - fade_start_x) / (right_limit - fade_start_x)).clamp(0.0, 1.0);
                if factor >= 0.9 {
                    skip_char = true;
                    default_color
                } else {
                    [
                        (default_color[0] as f32 + (bg_rgb[0] as f32 - default_color[0] as f32) * factor).round() as u8,
                        (default_color[1] as f32 + (bg_rgb[1] as f32 - default_color[1] as f32) * factor).round() as u8,
                        (default_color[2] as f32 + (bg_rgb[2] as f32 - default_color[2] as f32) * factor).round() as u8,
                    ]
                }
            } else {
                default_color
            };

            if !skip_char {
                ctx.text(chars[i].to_string(), cur_x, text_y, font_size, color);
                let c_w_ink = if is_monospace {
                    cell_width
                } else {
                    crate::widget::display::measure_text_width(&chars[i].to_string(), &font_family, font_size)
                };
                prev_char_end = offset + c_w_ink;
            }
        }

        ctx.text(
            "▼",
            x + w - 18.0,
            crate::layout::center_text_y(content.y, content.height, 10.0),
            10.0,
            [0x83, 0x83, 0x8a],
        );
    }

    /// Port of the legacy `keyboard_input` body.
    fn handle_key(&mut self, event: &crate::widget::KeyEvent) -> bool {
        if event.state != ElementState::Pressed {
            return false;
        }
        if !self.open || self.closing {
            if let Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) = event.logical_key {
                self.begin_open();
                let mut start_idx = self.selected;
                if start_idx < self.options.len() && self.options[start_idx] == "-" {
                    for i in 0..self.options.len() {
                        if self.options[i] != "-" {
                            start_idx = i;
                            break;
                        }
                    }
                }
                self.hovered_item = Some(start_idx);
                return true;
            }
            return false;
        }

        match event.logical_key {
            Key::Named(NamedKey::ArrowDown) => {
                let current = self.hovered_item.unwrap_or(self.selected);
                let mut next = (current + 1) % self.options.len();
                for _ in 0..self.options.len() {
                    if self.options[next] != "-" {
                        self.hovered_item = Some(next);
                        break;
                    }
                    next = (next + 1) % self.options.len();
                }
                true
            }
            Key::Named(NamedKey::ArrowUp) => {
                let current = self.hovered_item.unwrap_or(self.selected);
                let mut prev = if current == 0 { self.options.len() - 1 } else { current - 1 };
                for _ in 0..self.options.len() {
                    if self.options[prev] != "-" {
                        self.hovered_item = Some(prev);
                        break;
                    }
                    prev = if prev == 0 { self.options.len() - 1 } else { prev - 1 };
                }
                true
            }
            Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                if let Some(idx) = self.hovered_item {
                    if idx < self.options.len() && self.options[idx] != "-" {
                        if self.selected != idx || self.custom_display_text.is_some() {
                            self.selected = idx;
                            self.just_changed = true;
                        }
                        self.begin_close();
                    }
                }
                true
            }
            Key::Named(NamedKey::Escape) => {
                self.begin_close();
                true
            }
            _ => false,
        }
    }
}

impl Adapted<Dropdown> {
    /// Raised style: see the `raised` field.
    pub fn with_raised(mut self, raised: bool) -> Self {
        self.raised = raised;
        self
    }

    pub fn with_custom_display_text(mut self, text: &str) -> Self {
        self.custom_display_text = Some(text.to_string());
        self
    }

    pub fn with_font_family(mut self, font_family: &str) -> Self {
        self.font_family = font_family.to_string();
        self
    }

    pub fn with_open_upward(mut self, open_upward: bool) -> Self {
        self.open_upward = Some(open_upward);
        self
    }

    pub fn with_auto_width(mut self, auto_width: bool) -> Self {
        self.auto_width = auto_width;
        self
    }

    /// Popover geometry from the widget's laid-out rect — the legacy inherent
    /// `get_popover_geom` shape, for callers that hold the wrapper.
    pub fn get_popover_geom(&self) -> (f32, f32, f32, f32) {
        let (x, y, w, h) = WidgetHost::rect(self);
        let top = self.inner().label_top();
        self.inner().popover_geom(Rect { x, y: y + top, width: w, height: h - top })
    }
}

impl Layout for Dropdown {

    fn z_order(&self) -> i32 {
        if self.open {
            100
        } else {
            0
        }
    }

    fn inflates_label_rect(&self) -> bool {
        false
    }

    fn detached_label_inset(&self) -> f32 {
        4.0
    }

    /// Content size for the scene layout engine (Phase 2b). A normal dropdown is wide enough for
    /// the widest option (via `content_width`, which already includes the arrow/padding inset), so
    /// the control doesn't resize as the selection changes. A menu-button dropdown (fixed
    /// `custom_display_text`, e.g. a "File" menu) instead sizes to its display text — its label is
    /// fixed regardless of options, so fitting the widest entry would just stretch the trigger. The
    /// open popover still expands to the widest option via `popover_width`'s `.max(content_width())`.
    fn intrinsic_size(&self) -> Option<Size> {
        let width = if self.custom_display_text.is_some() {
            self.display_width()
        } else {
            self.content_width()
        };
        Some(Size::new(width, crate::layout::dropdown_height()))
    }

    fn intrinsic_measure_width(&self) -> bool {
        self.auto_width
    }
}

impl Paint for Dropdown {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::dropdown_corner_radius();
        if r > 0.0 {
            Some((r, (true, true, true, true)))
        } else {
            Some((r, (false, false, false, false)))
        }
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font_detached())
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        self.paint_background(rect, ctx);
        self.paint_text(rect, ctx);
    }

    fn popover(&self, rect: Rect) -> Option<(f32, f32, f32, f32)> {
        if self.open {
            // The ANIMATED unified box (trigger band + revealed menu), not the
            // full menu geometry: hosts replay popover text with bounds derived
            // from this rect (and the dl-text occlusion clamp reads it), so
            // reporting the drawn surface keeps labels — the band's included —
            // clipped to the traveling edge everywhere without per-app changes.
            Some(self.unified_geom_drawn(rect))
        } else {
            None
        }
    }

    fn draw_popover(&self, rect: Rect, pc: &mut dyn crate::layout::RenderTarget) {
        if !self.open {
            return;
        }

        // ONE continuous surface in the status-interface manner: the trigger
        // band grows into the menu — no detached popover plate, no drop
        // shadows. The unified box spans the trigger and the revealed menu;
        // the trigger's display text is redrawn on top of its band. Rows sit
        // at their FINAL positions (from the full geometry) and slide into
        // view as the traveling edge reveals them, clipped to the menu area by
        // hand (RenderTarget carries no clip stack); text clips through its
        // bounds.
        let (rx, ry, rw, _rh) = self.popover_geom(rect);
        let (ax, ay, aw, ah) = self.popover_geom_drawn(rect);
        if aw <= 0.5 || ah <= 0.5 {
            // Nothing revealed yet — the plain trigger stands alone.
            return;
        }
        let clip = |x: f32, y: f32, w: f32, h: f32| -> Option<(f32, f32, f32, f32)> {
            let x0 = x.max(ax);
            let y0 = y.max(ay);
            let x1 = (x + w).min(ax + aw);
            let y1 = (y + h).min(ay + ah);
            if x1 > x0 && y1 > y0 { Some((x0, y0, x1 - x0, y1 - y0)) } else { None }
        };

        let theme = colors::active_theme();
        let (ux, uy, uw, uh) = self.unified_geom_drawn(rect);

        // The ACTUAL button surface, expanded: the raised trigger's flush
        // inset plate grown over the unified box (real relief prims on a
        // PaintCtx-backed target; collector hosts degrade to a rounded fill).
        // The trigger's configured fill is usually transparent — the window
        // plate IS its face — so the expansion substitutes the opaque plate
        // color: the menu must cover the content beneath it.
        let radius = crate::layout::dropdown_corner_radius();
        let raw_bg = colors::dropdown_background_color();
        let face = if raw_bg[3] > 0.001 {
            let mut c = raw_bg;
            c[3] = 1.0;
            c
        } else {
            let mut c = crate::color::page_low_color();
            c[3] = 1.0;
            c
        };
        if self.raised {
            let depth = crate::layout::bevel_width().min(rect.height * 0.2);
            pc.inset_plate(face, ux, uy, uw, uh, radius, depth);
        } else {
            pc.rect_with_radius(self.border_color(), ux, uy, uw, uh, radius);
            pc.rect_with_radius(face, ux + 1.0, uy + 1.0, uw - 2.0, uh - 2.0, (radius - 1.0).max(0.0));
        }

        // Trigger content redrawn over its band (the box covers the widget-pass
        // trigger paint) — display text left, ▼ right, the paint_text palette.
        {
            let label_x = side_offset(&self.label);
            let (tx, ty) = (rect.x + label_x, rect.y);
            let (tw, th) = ((rect.width - label_x).max(0.0), rect.height);
            let band_bounds = Some([ux, uy, ux + uw, uy + uh]);
            let font = crate::layout::control_label_font_detached();
            let text_y = crate::layout::align_text_y(ty, th, 12.0, 0.0);
            pc.text_with_font_and_bounds(
                &self.display_text(),
                tx + 8.0,
                text_y,
                12.0,
                [0.8, 0.8, 0.85, 1.0],
                &font,
                band_bounds,
            );
            pc.text_with_font_and_bounds(
                "▼",
                tx + tw - 18.0,
                crate::layout::center_text_y(ty, th, 10.0),
                10.0,
                [0x83 as f32 / 255.0, 0x83 as f32 / 255.0, 0x8a as f32 / 255.0, 1.0],
                &font,
                band_bounds,
            );
        }

        if let Some(h_idx) = self.hovered_item {
            let iy = ry + h_idx as f32 * 24.0;
            // 4. Vibrantly colored translucent selection highlight
            if let Some((cx, cy, cw, ch)) = clip(rx + 2.0, iy + 2.0, rw - 4.0, 20.0) {
                pc.rect(theme.primary_accent, cx, cy, cw, ch);
            }
        }

        for (idx, opt) in self.options.iter().enumerate() {
            let iy = crate::layout::align_text_y(ry + idx as f32 * 24.0, 24.0, 12.0, 0.0);

            if opt == "-" {
                if let Some((cx, cy, cw, ch)) = clip(rx + 8.0, ry + idx as f32 * 24.0 + 11.5, rw - 16.0, 1.0) {
                    pc.rect(theme.surface_border, cx, cy, cw, ch);
                }
                continue;
            }

            let text_color = if self.hovered_item == Some(idx) {
                [0xff, 0xff, 0xff]
            } else if self.selected == idx {
                [0x3a, 0x9a, 0xff]
            } else {
                [0xcc, 0xcc, 0xd4]
            };

            let color_f32 = [
                text_color[0] as f32 / 255.0,
                text_color[1] as f32 / 255.0,
                text_color[2] as f32 / 255.0,
                1.0,
            ];

            // Bounds = the unified popover rect EXACTLY (not the menu sub-box):
            // the dl-text occlusion clamp exempts only exact-match overlay
            // labels, and the unified box's traveling edge clips identically.
            let bounds = Some([ux, uy, ux + uw, uy + uh]);
            let font = crate::layout::control_label_font_detached();
            pc.text_with_font_and_bounds(opt, rx + 8.0, iy, 12.0, color_f32, &font, bounds);
        }
    }
}

impl Input for Dropdown {
    /// The legacy geometric test: the widget rect (edges inclusive), extended to the open
    /// popover. `rect` is the full base rect (label strip included), as legacy `hit_test` used.
    fn hit(&self, rect: Rect, x: f32, y: f32) -> bool {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return false;
        }
        let hit_trigger =
            x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height;
        if self.open && !self.closing {
            let top = self.label_top();
            let content = Rect { x: rect.x, y: rect.y + top, width: rect.width, height: rect.height - top };
            let (rx, ry, rw, rh) = self.popover_geom(content);
            let hit_popover = x >= rx && x <= rx + rw && y >= ry && y <= ry + rh;
            hit_trigger || hit_popover
        } else {
            hit_trigger
        }
    }

    fn opens_context_menu(&self) -> bool {
        true
    }

    /// Ungated presses (legacy `mouse_input` saw every press): an open dropdown must close on
    /// an outside click it would otherwise never learn about.
    fn gates_presses(&self) -> bool {
        false
    }

    /// Wall-clock animation bookkeeping: report "changed" while a transition is
    /// in flight (drives redraws where the app's UiContext gets ticked) and
    /// settle a landed close.
    fn tick(&mut self, _dt: f32, _rect: Rect) -> bool {
        let animating = self.anim_start.is_some();
        self.settle_anim();
        animating
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        self.settle_anim();
        match event {
            Event::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                x: px,
                y: py,
                ..
            } => {
                let content = ectx.rect;
                let top = self.label_top();
                let (bx, by, bw, bh) = (content.x, content.y - top, content.width, content.height + top);
                let (rx, ry, rw, rh) = self.popover_geom(content);

                let inside_trigger = *px >= bx && *px <= bx + bw && *py >= by && *py <= by + bh;
                let inside_popover = self.open
                    && !self.closing
                    && *px >= rx && *px <= rx + rw && *py >= ry && *py <= ry + rh;

                if inside_popover {
                    let idx = ((py - ry) / 24.0) as usize;
                    if idx < self.options.len() {
                        if self.options[idx] == "-" {
                            return true;
                        }
                        if self.selected != idx || self.custom_display_text.is_some() {
                            self.selected = idx;
                            self.just_changed = true;
                        }
                    }
                    self.begin_close();
                    return true;
                }

                if inside_trigger {
                    if self.open && !self.closing {
                        self.begin_close();
                    } else {
                        self.begin_open();
                        // Animation frames arrive through the ctx tick loop.
                        if let Some(ui) = ectx.ui.as_deref_mut() {
                            ui.register_tick_receiver(ectx.id);
                        }
                        // Legacy `focus()` claimed only the global slot.
                        ectx.request_focus();
                    }
                    return true;
                }

                if self.open && !self.closing {
                    self.begin_close();
                    return true;
                }

                false
            }
            Event::PointerMove { x: px, y: py, .. } => {
                // The popover-item half of the legacy `on_cursor_moved`; the trigger-hover half
                // is the adapter's bookkeeping (MouseEnter/MouseLeave below).
                let was_hovered_item = self.hovered_item;
                self.hovered_item = None;
                if self.open && !self.closing {
                    let (rx, ry, rw, rh) = self.popover_geom(ectx.rect);
                    if *px >= rx && *px <= rx + rw && *py >= ry && *py <= ry + rh {
                        let idx = ((py - ry) / 24.0) as usize;
                        if idx < self.options.len() && self.options[idx] != "-" {
                            self.hovered_item = Some(idx);
                        }
                    }
                }
                self.hovered_item != was_hovered_item
            }
            Event::MouseEnter => {
                self.hovered = true;
                true
            }
            Event::MouseLeave => {
                self.hovered = false;
                true
            }
            Event::KeyInput(key_event) => {
                let handled = self.handle_key(key_event);
                if self.open {
                    if let Some(ui) = ectx.ui.as_deref_mut() {
                        ui.register_tick_receiver(ectx.id);
                    }
                }
                handled
            }
            Event::FocusIn => {
                // Legacy `focus()` claimed the global focus slot on every direct call
                // (test-interface focuses the ramp's preset dropdown this way).
                ectx.request_focus();
                false
            }
            Event::FocusOut => {
                // Legacy `unfocus` closed the dropdown.
                self.begin_close();
                false
            }
            _ => false,
        }
    }

    fn take_click(&mut self) -> bool {
        self.take_change()
    }

    fn take_change(&mut self) -> bool {
        self.take_change()
    }

    fn value(&self) -> i32 {
        self.selected as i32
    }

    fn value_string(&self) -> Option<String> {
        self.options.get(self.selected).cloned()
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        let val_trimmed = val.trim();
        for (idx, opt) in self.options.iter().enumerate() {
            if opt.eq_ignore_ascii_case(val_trimmed) {
                if self.selected != idx {
                    self.selected = idx;
                    self.just_changed = true;
                    return true;
                }
                return false;
            }
        }
        if let Ok(idx) = val_trimmed.parse::<usize>() {
            if idx < self.options.len() {
                if self.selected != idx {
                    self.selected = idx;
                    self.just_changed = true;
                    return true;
                }
                return false;
            }
        }
        false
    }
}

unsafe impl Send for Dropdown {}
unsafe impl Sync for Dropdown {}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::LayoutConstraints;

    #[test]
    fn test_dropdown_widget_interaction() {
        let mut dummy = crate::context::UiContext::new();
        let options = vec!["Option A".to_string(), "Option B".to_string(), "Option C".to_string()];
        let mut dd = Dropdown::new(options, 0);
        dd.set_rect(10.0, 10.0, 100.0, 24.0);

        // 1. Initial State
        assert!(!dd.open);
        assert_eq!(dd.selected, 0);

        // 2. Click trigger area opens dropdown
        let input_changed = dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 20.0, &mut dummy);
        assert!(input_changed);
        assert!(dd.open);

        // 3. Hovering options inside popover
        // Popover starts at y = 10 + 24 = 34. Options are of height 24 each.
        // Hover option B at y = 34 + 24 + 12 = 70.0
        let move_changed = dd.on_cursor_moved(50.0, 70.0, &mut dummy);
        assert!(move_changed);
        assert_eq!(dd.hovered_item, Some(1));

        // 4. Click option B selects it and starts the animated close
        let select_changed = dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 70.0, &mut dummy);
        assert!(select_changed);
        assert!(dd.closing, "selection starts the animated contraction");
        dd.land_anim_for_test();
        assert!(!dd.open);
        assert_eq!(dd.selected, 1);
        assert!(dd.take_change());
    }

    #[test]
    fn test_dropdown_context_menu_with_config() {
        let mut dummy = crate::context::UiContext::new();
        let options = vec!["Option A".to_string(), "Option B".to_string()];
        let mut dd = Dropdown::new(options, 0).with_config("path/to/config.json", "some_key");
        dd.set_rect(10.0, 10.0, 100.0, 24.0);

        assert!(!crate::widget::context_menu::is_visible());

        // Right click dropdown
        let handled = dd.mouse_input(MouseButton::Right, ElementState::Pressed, 50.0, 20.0, &mut dummy);
        assert!(handled);

        assert!(crate::widget::context_menu::is_visible());
        let menu_options = crate::widget::context_menu::options();
        assert!(menu_options.len() >= 3);
        assert_eq!(menu_options[1], "File: path/to/config.json");
        assert_eq!(menu_options[2], "Key: some_key");

        crate::widget::context_menu::hide();
        assert!(!crate::widget::context_menu::is_visible());
    }

    #[test]
    fn test_dropdown_separators() {
        let mut dummy = crate::context::UiContext::new();
        let options = vec![
            "Option A".to_string(),
            "-".to_string(),
            "Option B".to_string(),
        ];
        let mut dd = Dropdown::new(options, 0);
        dd.set_rect(10.0, 10.0, 100.0, 24.0);

        // Open dropdown
        dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 20.0, &mut dummy);
        assert!(dd.open);

        // Hover over separator at index 1 at y = 34 + 24 + 12 = 70.0
        dd.on_cursor_moved(50.0, 70.0, &mut dummy);
        assert_eq!(dd.hovered_item, None); // Separator should not be hovered

        // Click separator at index 1
        let clicked = dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 70.0, &mut dummy);
        assert!(clicked);
        assert!(dd.open); // Dropdown should remain open
        assert_eq!(dd.selected, 0); // Selection should not change

        // Hover over Option B at index 2 at y = 34 + 48 + 12 = 94.0
        dd.on_cursor_moved(50.0, 94.0, &mut dummy);
        assert_eq!(dd.hovered_item, Some(2));

        // Keyboard arrow up from index 2 should skip separator (index 1) and go to index 0
        let key_up = crate::widget::KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowUp),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
        };
        dd.keyboard_input(&key_up, &mut dummy);
        assert_eq!(dd.hovered_item, Some(0));
    }

    #[test]
    fn test_dropdown_ramp_parent_constraints() {
        let mut ramp = crate::widget::Ramp::new();
        // Set the rect of parent Ramp
        ramp.set_rect(20.0, 20.0, 410.0, 260.0);

        let options = vec![
            "Option 1".to_string(),
            "Option 2".to_string(),
            "Option 3".to_string(),
            "Option 4".to_string(),
            "Option 5".to_string(),
            "Option 6".to_string(),
        ];
        let mut dd = Dropdown::new(options, 0).with_label("Preset");
        dd.set_rect(30.0, 125.0, 110.0, 20.0);

        // Link the dropdown to the Ramp's read-data (the legacy direct-write path)
        dd.parent_snapshot = Some(ParentSnapshot {
            rect: crate::widget::WidgetHost::rect(&ramp),
            is_ramp: true,
            color: crate::widget::WidgetHost::color(&ramp),
        });

        // Compute geometry
        let (rx, ry, rw, rh) = dd.get_popover_geom();

        // Validate coordinates stay inside the parent Ramp bounds: x in [20, 430], y in [20, 280]
        assert!(rx >= 20.0, "rx {} should be >= 20.0", rx);
        assert!(rx + rw <= 430.0, "rx + rw {} should be <= 430.0", rx + rw);
        assert!(ry >= 20.0, "ry {} should be >= 20.0", ry);
        assert!(ry + rh <= 280.0, "ry + rh {} should be <= 280.0", ry + rh);
    }

    #[test]
    fn test_dropdown_label_fade_out() {
        let options = vec!["This is a very long option name that will exceed the dropdown width".to_string()];
        let mut dd = Dropdown::new(options, 0);
        dd.set_rect(10.0, 10.0, 100.0, 24.0); // very narrow dropdown

        let labels = dd.own_text_labels();
        // Labels are individual characters of selected_text, then the ▼ arrow (prim order).
        assert!(labels.len() > 2);

        // The last character label (excluding the arrow) should be faded (i.e. not the default color)
        let last_char_idx = labels.len() - 2;
        let first_char = &labels[0];
        let last_char = &labels[last_char_idx];

        let tc = colors::dropdown_text_color();
        let expected_color = [
            (colors::linear_to_srgb(tc[0]) * 255.0).round() as u8,
            (colors::linear_to_srgb(tc[1]) * 255.0).round() as u8,
            (colors::linear_to_srgb(tc[2]) * 255.0).round() as u8,
        ];
        assert_eq!(first_char.color, expected_color);
        assert_ne!(last_char.color, expected_color); // color has shifted towards background
    }

    #[test]
    fn test_dropdown_auto_width() {
        let dummy = crate::context::UiContext::new();
        let options = vec!["Short".to_string(), "A much longer option name".to_string()];
        let mut dd = Dropdown::new(options, 0).with_auto_width(true);
        dd.set_rect(10.0, 10.0, 50.0, 24.0);

        let size = dd.measure(LayoutConstraints::new(0.0, 500.0, 24.0, 24.0), &dummy);
        assert!(size.width > 50.0, "Measured auto-width {} should be greater than original width 50.0", size.width);

        let dd_no_auto = Dropdown::new(vec!["Short".to_string(), "A much longer option name".to_string()], 0);
        let size_no_auto = dd_no_auto.measure(LayoutConstraints::new(0.0, 500.0, 24.0, 24.0), &dummy);
        assert_eq!(size_no_auto.width, 0.0);
    }

    #[test]
    fn intrinsic_size_fits_widest_option() {
        let wide = Dropdown::new(
            vec!["Short".to_string(), "A much longer option name".to_string()],
            0,
        );
        let size = Layout::intrinsic_size(wide.inner()).expect("dropdown reports intrinsic size");
        assert!(size.width >= wide.content_width(), "width fits the widest option");
        assert_eq!(size.height, crate::layout::dropdown_height());

        let narrow = Dropdown::new(vec!["Hi".to_string()], 0);
        assert!(
            size.width > Layout::intrinsic_size(narrow.inner()).unwrap().width,
            "more/longer options measure wider",
        );
    }

    #[test]
    fn menu_button_trigger_fits_display_text_not_widest_option() {
        // A menu-button dropdown (fixed custom display text) sizes its trigger to that text, so a
        // long menu entry (e.g. a recent-file path) no longer stretches the "File" button.
        let menu = Dropdown::new(
            vec![
                "New".to_string(),
                "/home/user/some/very/long/recent/project/path".to_string(),
            ],
            0,
        )
        .with_custom_display_text("File");

        let trigger = Layout::intrinsic_size(menu.inner()).expect("dropdown reports intrinsic size");
        assert!(
            trigger.width < menu.inner().content_width(),
            "menu-button trigger ({}) fits its display text, not the widest option ({})",
            trigger.width,
            menu.inner().content_width(),
        );

        // The open popover still expands to the widest option.
        let content = Rect { x: 0.0, y: 0.0, width: trigger.width, height: trigger.height };
        assert!(
            menu.inner().popover_geom(content).2 >= menu.inner().content_width(),
            "popover still fits the widest option",
        );

        // The trigger leaves the paint pass's full budget (8px left pad + 28px right/arrow
        // reservation) for the label, so the display text renders without tripping the right-edge
        // fade. This is the paint condition `start_x + total_advance > right_limit` restated:
        // `content.x + 8 + advance > content.x + width - 28`, i.e. it must hold that
        // `width >= advance + 36`. Measured via `text_advance` — the same function the paint pass
        // lays out with — so the guarantee holds in a monospace UI font too.
        let (font_family, font_size) = crate::layout::control_label_font_detached_parsed();
        let advance = text_advance("File", &font_family, font_size);
        assert!(
            trigger.width >= advance + 36.0,
            "trigger width ({}) leaves room for the laid-out label (advance {} + 36px budget), \
             so it doesn't fade",
            trigger.width,
            advance,
        );
    }

    /// Migration additions: popover routing through the adapter (`WidgetHost::popover_rect` /
    /// `render_popover`), outside-press close, and Escape via routed key events.
    #[test]
    fn popover_reaches_hosts_through_the_adapter() {
        let mut dummy = crate::context::UiContext::new();
        let options = vec!["A".to_string(), "B".to_string()];
        let mut dd = Dropdown::new(options, 0);
        dd.set_rect(10.0, 10.0, 100.0, 24.0);

        assert!(WidgetHost::popover_rect(&dd).is_none(), "closed dropdown registers no popover");

        dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 20.0, &mut dummy);
        assert!(dd.open);
        // popover_rect reports the ANIMATED box — land the expansion first.
        dd.land_anim_for_test();
        let (rx, ry, rw, rh) = WidgetHost::popover_rect(&dd).expect("open dropdown registers its popover");
        assert_eq!((rx, ry), (10.0, 10.0), "the unified surface starts at the trigger band");
        assert!(rw >= 100.0 && rh == 72.0, "trigger band (24) + menu (2 * 24) as one box");

        // An outside press closes it (ungated presses — `gates_presses` is false).
        let closed = dd.mouse_input(MouseButton::Left, ElementState::Pressed, 500.0, 500.0, &mut dummy);
        assert!(closed);
        assert!(dd.closing, "outside press starts the animated contraction");
        dd.land_anim_for_test();
        assert!(!dd.open);
        assert!(!dd.take_change(), "outside close does not report a change");
    }
}
