//! The relief as a height field: the geometry the plate shader shades, sampled
//! per pixel and written out for fabrication.
//!
//! `shader2d.wgsl` never stores heights — it composes SLOPES per pixel and
//! lights them. But every slope it uses is the derivative of a height curve
//! this module integrates back: a plate is a slab whose face stands one roll
//! rise above the surface beneath it and whose perimeter roll descends toward
//! the silhouette; a carve cut into it (a CSG feature or a free recess, boss,
//! ridge, trough, groove or fillet) subtracts or adds its drop through the
//! same profile curve the shader's `carve_slope` differentiates. Heights are
//! relative to the local surface, so plates stack and carves etch — a
//! deboss, not a flat milling plane — exactly as the shader's composite
//! model says.
//!
//! Units: physical px on the z axis too (one px of drop is one px of run),
//! with the display metric ([`crate::units::metric`]) turning both into
//! millimetres at export. That is what makes a pinned `height=(mm)0.3` an
//! honest 0.3 mm here and a 1:1 print of the map a true relief. When the
//! metric is only *assumed* the sidecar says so; a fabrication tool should
//! refuse to trust it.
//!
//! Two ways in: `CCE_HEIGHTMAP=<file.png>` in any cce-ui client's
//! environment exports its third rendered frame (settled layout, first
//! metric known) and `CCE_HEIGHTMAP_MM=<mm per sample>` resamples to that
//! pitch; or an app calls [`request`] itself. Out comes a 16-bit greyscale
//! PNG (0 = the lowest point, 65535 = the highest) and a `<file>.json`
//! sidecar with the pitch, the range in mm, the datum and the metric's
//! source. Droplets and their scrims (decorative water) are skipped.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | `HeightField`: built from a frame's batches and features, read back in mm, resampled |
//! | `profiles` | the wall and roll height curves the shader differentiates, the rounded-rect SDF, smoothstep |
//! | `export` | asking for a height map (`request`, `CCE_HEIGHTMAP`) and writing the PNG and its sidecar |

mod export;
mod profiles;
#[cfg(test)]
mod tests;

pub use export::*;
use profiles::*;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use crate::backend::tessellate::DlBatch;
use crate::scene::relief_shade::{RECESS_DEPTH, ROLL_CUT};
use crate::units::MetricSource;


/// A sampled height field over one window, physical px on all three axes.
#[derive(Debug, Clone)]
pub struct HeightField {
    pub width: usize,
    pub height: usize,
    /// Physical px per millimetre of the display it was rendered for.
    pub px_per_mm: f32,
    pub source: MetricSource,
    /// Row-major heights in physical px, +z out of the screen, 0 = the
    /// window's base surface (what the root plate stands on).
    pub px: Vec<f32>,
}

/// Plate modes as `PlatePush::mode` carries them (see shader2d's `MODE_*`).
const MODE_PLATE: i32 = 1;

const MODE_RECESS: i32 = 2;

const MODE_BOSS: i32 = 3;

const MODE_RIDGE: i32 = 4;

const MODE_SPHERE: i32 = 5;

const MODE_FILLET_DOWN: i32 = 6;

const MODE_FILLET_UP: i32 = 7;

const MODE_GROOVE: i32 = 8;

const MODE_TROUGH: i32 = 9;

const MODE_ROLL: i32 = 11;

const MODE_LATTICE: i32 = 13;

const MODE_UNION: i32 = 14;

impl HeightField {
    /// Sample one frame's plate batches, in draw order, over a `width` ×
    /// `height` physical-px window rendered at `scale`. Batches that are not
    /// plates (plain geometry, text, images) have no height.
    pub fn from_frame(batches: &[DlBatch], features: &[[f32; 12]], width: usize, height: usize, scale: f32) -> Self {
        let metric = crate::units::metric();
        let mut hf = HeightField {
            width,
            height,
            px_per_mm: metric.physical_px_per_mm(),
            source: metric.source,
            px: vec![0.0; width * height],
        };
        let pinned_carve = crate::layout::bevel_height().map(|h| h * scale);
        let pinned_roll = crate::layout::roll_height().map(|h| h * scale);
        let mut profiles: Option<(f32, Profiles)> = None;
        for b in batches {
            let Some(p) = b.plate else { continue };
            let mode = p.mode.round() as i32;
            if !matches!(mode, 1..=9 | 11 | 13 | 14) {
                continue;
            }
            let shape = p.shape.clamp(2.0, 16.0);
            if profiles.as_ref().is_none_or(|(s, _)| (*s - shape).abs() > 1e-3) {
                profiles = Some((shape, Profiles::build(shape)));
            }
            let prof = &profiles.as_ref().unwrap().1;
            let t = p.light[3].max(0.001);
            // Pixel bounds: the shape's box plus its wall, clipped.
            let (mut x0, mut y0, mut x1, mut y1) = match mode {
                MODE_SPHERE => (p.rect[0] - p.rect[2], p.rect[1] - p.rect[2], p.rect[0] + p.rect[2], p.rect[1] + p.rect[2]),
                MODE_FILLET_DOWN | MODE_FILLET_UP => {
                    let r = p.rect[2] + t;
                    (p.rect[0] - r, p.rect[1] - r, p.rect[0] + r, p.rect[1] + r)
                }
                // A lattice's shading is bounded by its cover quad, which the
                // batch does not record; the one consumer (cce-grid) covers
                // its whole surface, so the window is the honest bound.
                MODE_GROOVE | MODE_LATTICE => (0.0, 0.0, width as f32, height as f32),
                // A union's push rect is its boxes' bounding box.
                MODE_UNION => (
                    p.rect[0] - p.rect[2] - t - 2.0,
                    p.rect[1] - p.rect[3] - t - 2.0,
                    p.rect[0] + p.rect[2] + t + 2.0,
                    p.rect[1] + p.rect[3] + t + 2.0,
                ),
                _ => (
                    p.rect[0] - p.rect[2] - t - 2.0,
                    p.rect[1] - p.rect[3] - t - 2.0,
                    p.rect[0] + p.rect[2] + t + 2.0,
                    p.rect[1] + p.rect[3] + t + 2.0,
                ),
            };
            if let Some(sc) = b.scissor {
                x0 = x0.max(sc.x * scale);
                y0 = y0.max(sc.y * scale);
                x1 = x1.min((sc.x + sc.width) * scale);
                y1 = y1.min((sc.y + sc.height) * scale);
            }
            let x0 = x0.floor().max(0.0) as usize;
            let y0 = y0.floor().max(0.0) as usize;
            let x1 = (x1.ceil().max(0.0) as usize).min(width);
            let y1 = (y1.ceil().max(0.0) as usize).min(height);
            if x0 >= x1 || y0 >= y1 {
                continue;
            }
            let (f_off, f_cnt) = (p.host[0].max(0.0) as usize, p.host[1].max(0.0) as usize);
            let carve_drop = pinned_carve.unwrap_or(RECESS_DEPTH * t);
            let roll_rise = pinned_roll.unwrap_or(t);
            for y in y0..y1 {
                for x in x0..x1 {
                    let pt = (x as f32 + 0.5, y as f32 + 0.5);
                    if let Some(cr) = b.clip_rrect {
                        if rr_sdf(pt, [cr[0], cr[1], cr[2], cr[3]], [cr[4]; 4], 2.0, t) > 0.0 {
                            continue;
                        }
                    }
                    let dh = match mode {
                        MODE_PLATE => {
                            // Positive inside, like the shader's `d`.
                            let d = -rr_sdf(pt, p.rect, p.radii, shape, t);
                            if d <= 0.0 {
                                continue;
                            }
                            let f = 1.0 - (d / t).clamp(0.0, 1.0);
                            let mut h = roll_rise * prof.roll_height(f);
                            for feat in features.iter().skip(f_off).take(f_cnt) {
                                let ft = feat[8].max(0.001);
                                let fd = rr_sdf(pt, [feat[0], feat[1], feat[2], feat[3]], [feat[4], feat[5], feat[6], feat[7]], shape, ft);
                                let v = (-fd / ft + 0.5).clamp(0.0, 1.0);
                                if v > 0.0 {
                                    // params.y: positive carves down, negative
                                    // raises a boss.
                                    h -= feat[9] * prof.carve_height(v);
                                }
                            }
                            h
                        }
                        MODE_ROLL => {
                            let d = -rr_sdf(pt, p.rect, p.radii, shape, t);
                            if d <= 0.0 {
                                continue;
                            }
                            let f = 1.0 - (d / t).clamp(0.0, 1.0);
                            roll_rise * (prof.roll_height(f) - 1.0)
                        }
                        MODE_SPHERE => {
                            let (dx, dy) = (pt.0 - p.rect[0], pt.1 - p.rect[1]);
                            let r2 = p.rect[2] * p.rect[2];
                            let d2 = dx * dx + dy * dy;
                            if d2 >= r2 {
                                continue;
                            }
                            (r2 - d2).sqrt()
                        }
                        MODE_GROOVE => {
                            let s = (pt.0 - p.rect[0]) * p.radii[0] + (pt.1 - p.rect[1]) * p.radii[1];
                            // Positive outside the band; the line is the low side.
                            let fd = s.abs() - p.rect[2];
                            let u = (fd / t + 0.5).clamp(0.0, 1.0);
                            -carve_drop * (1.0 - prof.carve_height(u))
                        }
                        MODE_LATTICE => {
                            // Fold into the period about one cell's centre and
                            // measure that cell — the union distance of every
                            // well (see the shader's MODE_LATTICE). Positive
                            // outside the cell; the wall runs from the edge
                            // outward over t, floor at the edge.
                            // Mitred, not offset: the wall's outer edge is the
                            // cell grown by t with SHARP corners (crest lines
                            // meet at a crossing as hips), and u is the
                            // fraction across the band between the two
                            // contours (1 at the cell edge, 0 at the outer).
                            let (px, py) = (p.host[0].max(1e-3), p.host[1].max(1e-3));
                            let c = (pt.0 - p.rect[0], pt.1 - p.rect[1]);
                            let c = (c.0 - px * (c.0 / px).round(), c.1 - py * (c.1 / py).round());
                            let d_in = rr_sdf(c, [0.0, 0.0, p.rect[2], p.rect[3]], p.radii, shape, t);
                            let d_out = rr_sdf(c, [0.0, 0.0, p.rect[2] + t, p.rect[3] + t], [0.0; 4], shape, t);
                            let frac = (d_in / (d_in - d_out).max(1e-3)).clamp(0.0, 1.0);
                            -carve_drop * prof.carve_height(1.0 - frac)
                        }
                        MODE_UNION => {
                            // Mitred per box (band between the box shrunk and
                            // grown by t/2 at its own radius), union = the box
                            // the point is deepest in (see the shader's
                            // MODE_UNION). One profile.
                            let hw = 0.5 * t;
                            let mut best = f32::MIN;
                            for feat in features.iter().skip(f_off).take(f_cnt) {
                                let radii = [feat[4], feat[5], feat[6], feat[7]];
                                let inner = [feat[0], feat[1], (feat[2] - hw).max(0.5), (feat[3] - hw).max(0.5)];
                                let outer = [feat[0], feat[1], feat[2] + hw, feat[3] + hw];
                                let d_in = rr_sdf(pt, inner, radii, shape, t);
                                let d_out = rr_sdf(pt, outer, radii, shape, t);
                                let frac = (d_in / (d_in - d_out).max(1e-3)).clamp(0.0, 1.0);
                                best = best.max((0.5 - frac) * t);
                            }
                            if best == f32::MIN {
                                continue;
                            }
                            let u = (best / t + 0.5).clamp(0.0, 1.0);
                            let sign = if p.radii[0] > 0.5 { 1.0 } else { -1.0 };
                            sign * carve_drop * prof.carve_height(u)
                        }
                        MODE_FILLET_DOWN | MODE_FILLET_UP => {
                            let (cx, cy) = (pt.0 - p.rect[0], pt.1 - p.rect[1]);
                            let dist = (cx * cx + cy * cy).sqrt().max(1e-4);
                            let ang = cy.atan2(cx);
                            let a0 = p.radii[0];
                            let rel = (ang - a0).rem_euclid(std::f32::consts::TAU);
                            if rel > std::f32::consts::FRAC_PI_2 {
                                continue;
                            }
                            // Positive outside the arc's circle; outside is
                            // the low side of a recessed fillet.
                            let fd = dist - p.rect[2];
                            let u = (fd / t + 0.5).clamp(0.0, 1.0);
                            let sign = if mode == MODE_FILLET_DOWN { -1.0 } else { 1.0 };
                            sign * carve_drop * prof.carve_height(u)
                        }
                        _ => {
                            // Recess, boss, ridge, trough: the box step, u = 1
                            // deep inside.
                            let d = -rr_sdf(pt, p.rect, p.radii, shape, t);
                            let u = (d / t + 0.5).clamp(0.0, 1.0);
                            match mode {
                                MODE_RECESS => -carve_drop * prof.carve_height(u),
                                MODE_BOSS => carve_drop * prof.carve_height(u),
                                MODE_RIDGE | MODE_TROUGH => {
                                    let w = (2.0 * u).min(2.0 - 2.0 * u).clamp(0.0, 1.0);
                                    let up = if mode == MODE_RIDGE { 1.0 } else { -1.0 };
                                    up * 0.5 * carve_drop * prof.carve_height(w)
                                }
                                _ => 0.0,
                            }
                        }
                    };
                    hf.px[y * width + x] += dh;
                }
            }
        }
        hf
    }

    /// Height in mm at a sample.
    pub fn mm_at(&self, x: usize, y: usize) -> f32 {
        self.px[y * self.width + x] / self.px_per_mm
    }

    /// (lowest, highest) in px.
    pub fn range_px(&self) -> (f32, f32) {
        self.px.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &v| (lo.min(v), hi.max(v)))
    }

    /// The field resampled to `mm_per_sample` (bilinear), or a copy at the
    /// native pitch.
    pub fn resampled(&self, mm_per_sample: Option<f32>) -> (Vec<f32>, usize, usize, f32) {
        let Some(mm) = mm_per_sample.filter(|m| *m > 0.0) else {
            return (self.px.clone(), self.width, self.height, 1.0 / self.px_per_mm);
        };
        let step = mm * self.px_per_mm; // source px per output sample
        let w = ((self.width as f32 / step).round() as usize).max(1);
        let h = ((self.height as f32 / step).round() as usize).max(1);
        let mut out = Vec::with_capacity(w * h);
        for j in 0..h {
            for i in 0..w {
                let sx = ((i as f32 + 0.5) * step - 0.5).clamp(0.0, (self.width - 1) as f32);
                let sy = ((j as f32 + 0.5) * step - 0.5).clamp(0.0, (self.height - 1) as f32);
                let (x0, y0) = (sx.floor() as usize, sy.floor() as usize);
                let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
                let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
                let at = |x: usize, y: usize| self.px[y * self.width + x];
                let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * fx;
                let bot = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * fx;
                out.push(top + (bot - top) * fy);
            }
        }
        (out, w, h, mm)
    }
}
