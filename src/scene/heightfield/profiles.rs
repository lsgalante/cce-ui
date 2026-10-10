//! The height curves: a carve wall's and a plate roll's, tabulated from the same profiles the
//! shader's `carve_slope` and roll differentiate, and the rounded-rect SDF and smoothstep the
//! height field samples them through.

use super::*;

/// The height curves the shader differentiates, tabulated once per export.
pub(super) struct Profiles {
    /// Carve: 0 on the plateau (v = 0) → 1 on the floor (v = 1).
    carve: Vec<f32>,
    /// Roll: 1 at the face join (f = 0) → the rim's remaining height at the
    /// silhouette (f = 1), unit rise.
    roll: Vec<f32>,
}

pub(super) const TABLE: usize = 256;

impl Profiles {
    pub(super) fn build(shape: f32) -> Self {
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

    pub(super) fn sample(table: &[f32], x: f32) -> f32 {
        let xs = x.clamp(0.0, 1.0) * TABLE as f32;
        let i0 = (xs.floor() as usize).min(TABLE - 1);
        let fr = xs - i0 as f32;
        table[i0] + (table[i0 + 1] - table[i0]) * fr
    }

    pub(super) fn carve_height(&self, v: f32) -> f32 {
        Self::sample(&self.carve, v)
    }

    pub(super) fn roll_height(&self, f: f32) -> f32 {
        Self::sample(&self.roll, f)
    }
}

/// Signed distance to a rounded box, positive outside — the distance part of
/// the shader's `rr_sdf_grad`, superellipse corners and their first-order
/// refinement included, so the sampled walls sit where the shaded ones do.
pub(super) fn rr_sdf(p: (f32, f32), rect: [f32; 4], radii: [f32; 4], shape: f32, roll: f32) -> f32 {
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

pub(super) fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
