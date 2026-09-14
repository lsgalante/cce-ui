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

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use crate::backend::window_runner::DlBatch;
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

/// One export request: where to write, and at what pitch (`None` = one
/// sample per physical px).
#[derive(Debug, Clone)]
pub struct Request {
    pub path: PathBuf,
    pub mm_per_sample: Option<f32>,
}

static REQUEST: Mutex<Option<Request>> = Mutex::new(None);
static FRAMES: AtomicU32 = AtomicU32::new(0);
/// The frame the environment request fires on: layout has settled and the
/// output metric has arrived by then.
const ENV_FRAME: u32 = 3;

/// Ask the runner to export the next frame's height field.
pub fn request(path: impl Into<PathBuf>, mm_per_sample: Option<f32>) {
    if let Ok(mut r) = REQUEST.lock() {
        *r = Some(Request { path: path.into(), mm_per_sample });
    }
}

/// The runner's per-frame poll: an explicit [`request`], or the environment's
/// `CCE_HEIGHTMAP` once, on frame [`ENV_FRAME`].
pub(crate) fn take_request() -> Option<Request> {
    let n = FRAMES.fetch_add(1, Ordering::Relaxed) + 1;
    if n == ENV_FRAME {
        if let Ok(path) = std::env::var("CCE_HEIGHTMAP") {
            if !path.is_empty() {
                let mm = std::env::var("CCE_HEIGHTMAP_MM").ok().and_then(|v| v.parse::<f32>().ok()).filter(|v| *v > 0.0);
                request(path, mm);
            }
        }
    }
    REQUEST.lock().ok().and_then(|mut r| r.take())
}

/// The height curves the shader differentiates, tabulated once per export.
struct Profiles {
    /// Carve: 0 on the plateau (v = 0) → 1 on the floor (v = 1).
    carve: Vec<f32>,
    /// Roll: 1 at the face join (f = 0) → the rim's remaining height at the
    /// silhouette (f = 1), unit rise.
    roll: Vec<f32>,
}

const TABLE: usize = 256;

impl Profiles {
    fn build(shape: f32) -> Self {
        let carve_lut = crate::layout::bevel_profile_slopes();
        let roll_lut = crate::layout::roll_profile_slopes();
        let n = crate::layout::BEVEL_PROFILE_SAMPLES as f32;
        // The shader's LUT sampling, slopes with its end tapers, integrated.
        let lut_slope = |lut: &[f32], x: f32, both_ends: bool| -> f32 {
            let xc = x.clamp(0.0, 1.0);
            let xs = (xc * n - 0.5).clamp(0.0, n - 1.0);
            let i0 = xs.floor() as usize;
            let i1 = (i0 + 1).min(lut.len() - 1);
            let fr = xs - xs.floor();
            let win = if both_ends {
                (xc.min(1.0 - xc) * n * 0.667).clamp(0.0, 1.0)
            } else {
                (xc * n * 0.667).clamp(0.0, 1.0)
            };
            (lut[i0] + (lut[i1] - lut[i0]) * fr) * win
        };
        let mut carve = Vec::with_capacity(TABLE + 1);
        let mut roll = Vec::with_capacity(TABLE + 1);
        let (mut hc, mut hr) = (0.0f32, 1.0f32);
        for i in 0..=TABLE {
            let x = i as f32 / TABLE as f32;
            match &carve_lut {
                Some(lut) => {
                    if i > 0 {
                        let xm = (i as f32 - 0.5) / TABLE as f32;
                        hc += lut_slope(lut, xm, true) / TABLE as f32;
                    }
                    carve.push(hc);
                }
                None => carve.push(if shape > 2.001 {
                    x * x * x * (x * (x * 6.0 - 15.0) + 10.0)
                } else {
                    x * x * (3.0 - 2.0 * x)
                }),
            }
            match &roll_lut {
                Some(lut) => {
                    if i > 0 {
                        let xm = (i as f32 - 0.5) / TABLE as f32;
                        hr -= lut_slope(lut, xm, false) / TABLE as f32;
                    }
                    roll.push(hr);
                }
                None => {
                    let fc = x * ROLL_CUT;
                    roll.push(if shape > 2.001 {
                        (1.0 - fc.powf(shape)).max(0.0).powf(1.0 / shape)
                    } else {
                        (1.0 - fc * fc).max(0.0).sqrt()
                    })
                }
            }
        }
        Profiles { carve, roll }
    }

    fn sample(table: &[f32], x: f32) -> f32 {
        let xs = x.clamp(0.0, 1.0) * TABLE as f32;
        let i0 = (xs.floor() as usize).min(TABLE - 1);
        let fr = xs - i0 as f32;
        table[i0] + (table[i0 + 1] - table[i0]) * fr
    }

    fn carve_height(&self, v: f32) -> f32 {
        Self::sample(&self.carve, v)
    }

    fn roll_height(&self, f: f32) -> f32 {
        Self::sample(&self.roll, f)
    }
}

/// Signed distance to a rounded box, positive outside — the distance part of
/// the shader's `rr_sdf_grad`, superellipse corners and their first-order
/// refinement included, so the sampled walls sit where the shaded ones do.
fn rr_sdf(p: (f32, f32), rect: [f32; 4], radii: [f32; 4], shape: f32, roll: f32) -> f32 {
    let c = (p.0 - rect[0], p.1 - rect[1]);
    let (r_lo, r_hi) = if c.0 > 0.0 { (radii[1], radii[2]) } else { (radii[0], radii[3]) };
    let r = if c.1 > 0.0 { r_hi } else { r_lo };
    let q = (c.0.abs() - rect[2] + r, c.1.abs() - rect[3] + r);
    if q.0 > 0.0 && q.1 > 0.0 {
        if shape > 2.001 {
            let lp = (q.0.powf(shape) + q.1.powf(shape)).powf(1.0 / shape).max(1e-4);
            let g = ((q.0 / lp).powf(shape - 1.0), (q.1 / lp).powf(shape - 1.0));
            let gm = (g.0 * g.0 + g.1 * g.1).sqrt().max(1e-4);
            let d0 = (lp - r) / gm;
            let dir = (g.0 / gm, g.1 / gm);
            let roll = roll.max(2.0);
            let step = d0.clamp(-roll, roll);
            let q1 = ((q.0 - step * dir.0).max(1e-4), (q.1 - step * dir.1).max(1e-4));
            let lp1 = (q1.0.powf(shape) + q1.1.powf(shape)).powf(1.0 / shape).max(1e-4);
            let g1 = ((q1.0 / lp1).powf(shape - 1.0), (q1.1 / lp1).powf(shape - 1.0));
            let gm1 = (g1.0 * g1.0 + g1.1 * g1.1).sqrt().max(1e-4);
            let d1 = step + (lp1 - r) / gm1;
            let w = smoothstep(roll, roll * 1.5 + 2.0, d0.abs());
            return d1 + (d0 - d1) * w;
        }
        let len = (q.0 * q.0 + q.1 * q.1).sqrt().max(1e-4);
        return len - r;
    }
    if q.0 > q.1 { q.0 - r } else { q.1 - r }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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
            if profiles.as_ref().map_or(true, |(s, _)| (*s - shape).abs() > 1e-3) {
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
                            // cell grown by t at the SAME corner radius, and u
                            // is the fraction across the band between the two
                            // contours (1 at the cell edge, 0 at the outer).
                            let (px, py) = (p.host[0].max(1e-3), p.host[1].max(1e-3));
                            let c = (pt.0 - p.rect[0], pt.1 - p.rect[1]);
                            let c = (c.0 - px * (c.0 / px).round(), c.1 - py * (c.1 / py).round());
                            let d_in = rr_sdf(c, [0.0, 0.0, p.rect[2], p.rect[3]], p.radii, shape, t);
                            let d_out = rr_sdf(c, [0.0, 0.0, p.rect[2] + t, p.rect[3] + t], p.radii, shape, t);
                            let frac = (d_in / (d_in - d_out).max(1e-3)).clamp(0.0, 1.0);
                            -carve_drop * prof.carve_height(1.0 - frac)
                        }
                        MODE_UNION => {
                            // Nearest box of the run — the union SDF — through
                            // one profile (see the shader's MODE_UNION).
                            let mut best = f32::MAX;
                            for feat in features.iter().skip(f_off).take(f_cnt) {
                                let fd = rr_sdf(pt, [feat[0], feat[1], feat[2], feat[3]], [feat[4], feat[5], feat[6], feat[7]], shape, t);
                                best = best.min(fd);
                            }
                            if best == f32::MAX {
                                continue;
                            }
                            let u = (-best / t + 0.5).clamp(0.0, 1.0);
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

/// Write the field as a 16-bit greyscale PNG (0 = lowest, 65535 = highest)
/// plus a `<path>.json` sidecar carrying what the PNG cannot: the sample
/// pitch and the height range in mm, the datum, and whether the metric
/// behind those millimetres was measured or only assumed.
pub fn export_png(hf: &HeightField, path: &Path, mm_per_sample: Option<f32>) -> std::io::Result<()> {
    let (data, w, h, pitch_mm) = hf.resampled(mm_per_sample);
    let (lo, hi) = data.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    let span = (hi - lo).max(1e-6);
    let mut bytes = Vec::with_capacity(w * h * 2);
    for v in &data {
        let q = (((v - lo) / span) * 65535.0).round().clamp(0.0, 65535.0) as u16;
        bytes.extend_from_slice(&q.to_be_bytes());
    }
    let file = std::fs::File::create(path)?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Grayscale);
    enc.set_depth(png::BitDepth::Sixteen);
    let mut writer = enc.write_header().map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    writer.write_image_data(&bytes).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    writer.finish().map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let side = serde_json::json!({
        "width": w,
        "height": h,
        "mm_per_sample": pitch_mm,
        "px_per_mm_physical": hf.px_per_mm,
        "min_mm": lo / hf.px_per_mm,
        "max_mm": hi / hf.px_per_mm,
        "datum": "0 = the window's base surface; values are heights above it, +z out of the screen",
        "png": "16-bit greyscale, 0 = min_mm, 65535 = max_mm, linear",
        "metric_source": hf.source.as_str(),
        "metric_is_real": matches!(hf.source, MetricSource::Measured | MetricSource::Configured),
    });
    let mut side_path = path.as_os_str().to_owned();
    side_path.push(".json");
    std::fs::write(side_path, serde_json::to_string_pretty(&side).unwrap_or_default())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vk::PlatePush;

    fn plate(rect: [f32; 4], t: f32, host: [f32; 4]) -> DlBatch {
        DlBatch {
            scissor: None,
            clip_rrect: None,
            start: 0,
            end: 0,
            plate: Some(PlatePush {
                rect,
                radii: [4.0; 4],
                light: [0.0, 0.0, 1.0, t],
                material: [1.0, 0.4, 24.0, 0.2],
                host,
                specular_tint: [1.0; 4],
                mode: 1.0,
                shape: 2.0,
            }),
            blur_behind: false,
        }
    }

    #[test]
    fn a_plate_stands_one_rise_above_nothing_and_rolls_to_its_rim() {
        // 100×100 window, plate 60×60 centred, roll 10.
        let b = plate([50.0, 50.0, 30.0, 30.0], 10.0, [0.0; 4]);
        let hf = HeightField::from_frame(&[b], &[], 100, 100, 1.0);
        let face = hf.px[50 * 100 + 50];
        assert!((face - 10.0).abs() < 1e-3, "face {face}");
        assert_eq!(hf.px[5 * 100 + 5], 0.0, "outside the plate is the base");
        // Just inside the silhouette (pixel centre 21.5, silhouette at 20:
        // d = 1.5, f = 0.85) the rim is cut off at the ROLL_CUT height.
        let rim = hf.px[50 * 100 + 21];
        let expected = 10.0 * (1.0 - (ROLL_CUT * 0.85f32).powi(2)).sqrt();
        assert!((rim - expected).abs() < 0.3, "rim {rim} vs {expected}");
        assert!(rim < face);
    }

    #[test]
    fn a_csg_recess_etches_its_depth_into_the_face() {
        let b = plate([50.0, 50.0, 40.0, 40.0], 8.0, [0.0, 1.0, 0.0, 0.0]);
        // A 20×20 recess at the centre, wall 6, dropping 4.
        let feat = [50.0, 50.0, 10.0, 10.0, 2.0, 2.0, 2.0, 2.0, 6.0, 4.0, 0.0, 0.0];
        let hf = HeightField::from_frame(&[b], &[feat], 100, 100, 1.0);
        assert!((hf.px[50 * 100 + 50] - 4.0).abs() < 1e-3, "floor {}", hf.px[50 * 100 + 50]);
        assert!((hf.px[50 * 100 + 75] - 8.0).abs() < 1e-3, "face {}", hf.px[50 * 100 + 75]);
        // The wall lands between.
        let wall = hf.px[50 * 100 + 60];
        assert!(wall > 4.0 && wall < 8.0, "wall {wall}");
    }

    #[test]
    fn a_free_recess_carves_the_analytic_ratio_of_its_wall() {
        let mut b = plate([50.0, 50.0, 20.0, 20.0], 10.0, [-1e5, -1e5, 1e5, 1e5]);
        b.plate.as_mut().unwrap().mode = 2.0;
        let hf = HeightField::from_frame(&[b], &[], 100, 100, 1.0);
        let floor = hf.px[50 * 100 + 50];
        assert!((floor + RECESS_DEPTH * 10.0).abs() < 1e-3, "floor {floor}");
        assert_eq!(hf.px[5 * 100 + 5], 0.0);
    }

    #[test]
    fn a_lattice_is_one_surface_with_plateau_rails_and_mitred_crossings() {
        // Period 50, cells 30 wide (gap 20), wall 10 = the half-gap, one
        // cell centred at (25, 25). p_rect = cell centre + half-extents,
        // p_host.xy = period, radii 4 (the fixture's).
        let mut b = plate([25.0, 25.0, 15.0, 15.0], 10.0, [50.0, 50.0, 1e6, 1e6]);
        b.plate.as_mut().unwrap().mode = 13.0;
        let hf = HeightField::from_frame(&[b], &[], 100, 100, 1.0);
        let at = |x: usize, y: usize| hf.px[y * 100 + x];
        let drop = RECESS_DEPTH * 10.0;
        // Every cell floor, not just the one the push names: (25,25) and
        // its period neighbour (75,75).
        assert!((at(25, 25) + drop).abs() < 1e-3, "floor {}", at(25, 25));
        assert!((at(75, 75) + drop).abs() < 1e-3, "neighbour floor {}", at(75, 75));
        // The rail centre line between two cells is exactly one run from
        // either edge: plateau, not a doubled wall. Pixel centres straddle
        // the line by half a px (49.5 and 50.5 are each 9.5 from a cell), so
        // the two samples sit a hair into opposite walls — equal, and within
        // the wall's first half-px of drop.
        assert!((at(49, 25) - at(50, 25)).abs() < 1e-3, "rail centre symmetric {} {}", at(49, 25), at(50, 25));
        assert!(at(50, 25) > -0.1 && at(50, 25) <= 0.0, "rail centre {}", at(50, 25));
        assert!(at(50, 25) > at(45, 25), "rail centre above the wall");
        // The crossing where four cells meet is outside every cell's outer
        // contour: plateau — the mitre the per-cell rings never gave.
        assert!(at(50, 50).abs() < 1e-3, "crossing {}", at(50, 50));
        // The outer contour keeps the CELL's corner radius (4), not radius +
        // run (14): the wall reaches to within a corner's rounding of the
        // crossing centre. (48.5, 48.5) sits 0.46 px inside the outer box's
        // corner arc — a hair of carve — where an offset-curve outer contour
        // (13.7 px from the cell, past the 10 px run) would be flat.
        assert!(at(48, 48) < 0.0 && at(48, 48) > -0.15, "near-crossing {}", at(48, 48));
        // On the rail centre line the outer contour is straight: plateau all
        // the way up to the crossing's corner rounding (the same half-px
        // straddle as above: 49.5 and 50.5 sit a hair into opposite walls).
        assert!((at(49, 45) - at(50, 45)).abs() < 1e-3, "rail centre near crossing symmetric");
        assert!(at(50, 45) > -0.1 && at(50, 45) <= 0.0, "rail centre near crossing {}", at(50, 45));
        // Halfway out the wall is between floor and plateau, on both sides
        // of the rail (one wall from each cell, symmetric). Pixel centres:
        // 45.5 is 5.5 past the first cell's edge at 40, 54.5 is 5.5 before
        // the neighbour's edge at 60.
        let w1 = at(45, 25);
        let w2 = at(54, 25);
        assert!(w1 < 0.0 && w1 > -drop, "wall {w1}");
        assert!((w1 - w2).abs() < 1e-3, "walls symmetric {w1} {w2}");
    }

    #[test]
    fn a_carve_union_is_one_wall_around_the_union_of_its_boxes() {
        // An L: a 60×20 bar across the top and a 20×60 bar down the left,
        // sharing the corner square (10..30). Wall 8, radii 4 (fixture).
        let bar_h = [40.0, 20.0, 30.0, 10.0, 4.0, 4.0, 4.0, 4.0, 8.0, 0.0, 0.0, 0.0];
        let bar_v = [20.0, 40.0, 10.0, 30.0, 4.0, 4.0, 4.0, 4.0, 8.0, 0.0, 0.0, 0.0];
        let mut b = plate([40.0, 40.0, 30.0, 30.0], 8.0, [0.0, 2.0, 1e6, 1e6]);
        b.plate.as_mut().unwrap().mode = 14.0;
        b.plate.as_mut().unwrap().radii = [0.0; 4];
        let hf = HeightField::from_frame(&[b], &[bar_h, bar_v], 100, 100, 1.0);
        let at = |x: usize, y: usize| hf.px[y * 100 + x];
        let drop = RECESS_DEPTH * 8.0;
        // Deep inside either bar: the full drop, once.
        assert!((at(55, 20) + drop).abs() < 1e-3, "top bar {}", at(55, 20));
        assert!((at(20, 55) + drop).abs() < 1e-3, "left bar {}", at(20, 55));
        // The shared corner square lies inside BOTH boxes. With one recess
        // per box, each box's wall would run straight through the other's
        // interior here (the vertical bar's right wall at x = 30 crosses the
        // top bar). As a union the interior is flat floor: exactly one drop.
        assert!((at(25, 20) + drop).abs() < 1e-3, "corner interior {}", at(25, 20));
        assert!((at(20, 25) + drop).abs() < 1e-3, "corner interior {}", at(20, 25));
        // Well outside: the base.
        assert_eq!(at(80, 80), 0.0);
        // The wall straddles the union outline by ±t/2 = 4: sampled 2 px
        // outside the top bar's lower edge (y = 30), on the wall.
        let wall = at(55, 32);
        assert!(wall < 0.0 && wall > -drop, "wall {wall}");
    }

    #[test]
    fn resample_keeps_the_face_height() {
        let b = plate([50.0, 50.0, 40.0, 40.0], 8.0, [0.0; 4]);
        let mut hf = HeightField::from_frame(&[b], &[], 100, 100, 1.0);
        hf.px_per_mm = 10.0; // 10 px per mm → 100 px = 10 mm
        let (data, w, h, pitch) = hf.resampled(Some(0.5));
        assert_eq!((w, h), (20, 20));
        assert!((pitch - 0.5).abs() < 1e-6);
        assert!((data[10 * 20 + 10] - 8.0).abs() < 1e-3);
    }

    #[test]
    fn export_writes_png_and_sidecar() {
        let b = plate([50.0, 50.0, 40.0, 40.0], 8.0, [0.0; 4]);
        let hf = HeightField::from_frame(&[b], &[], 100, 100, 1.0);
        let dir = std::env::temp_dir().join(format!("cce-heightfield-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("map.png");
        export_png(&hf, &path, None).unwrap();
        let png = std::fs::read(&path).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        let side: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("map.png.json")).unwrap()).unwrap();
        assert_eq!(side["width"], 100);
        assert_eq!(side["metric_source"], "assumed");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
