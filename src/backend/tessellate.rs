//! The tessellator: a `DisplayList` (or the legacy primitive tuples) turned
//! into `Vertex` batches with their push constants, plus the vertex helpers
//! apps call directly. Platform-neutral — it produces the data the renderer
//! draws and knows nothing of the window system; moved out of
//! `window_runner` so another shell can share it.

use crate::widget::WidgetHost;
use crate::draw::Batch2D;

/// A droplet spec resolved against a concrete rect: the push-constant fields
/// that define its SILHOUETTE, in logical px.
///
/// Shared by [`crate::scene::paint::Prim::Droplet`] and
/// [`crate::scene::paint::Prim::DropletScrim`] so the lit drop and the vignette
/// drawn inside it can never disagree about the shape — the whole reason the
/// scrim rides the droplet's shader path instead of approximating the outline
/// with a rounded rect.
struct DropletGeom {
    hx: f32,
    hy: f32,
    sag: f32,
    br: f32,
    bw: f32,
    k: f32,
    sr: f32,
    ar: f32,
    band: f32,
    bow: f32,
    /// How far the contact shadow reaches below/beside the box (0 when the
    /// spec has no shadow). The lit drop's cover quad grows by this; a scrim
    /// never draws outside the silhouette and ignores it.
    sh_reach: f32,
}

fn droplet_geom(rect: &crate::scene::layout::Rect, spec: &crate::scene::paint::DropletSpec) -> DropletGeom {
    let hx = rect.width * 0.5;
    let hy = rect.height * 0.5;
    let sag = spec.sag.clamp(0.0, 0.9) * rect.height;
    // belly <= 0 disables the belly outright (the oval-dewdrop default) — the
    // shader skips the smin when the radius is 0.
    let (br, bw) = if spec.belly > 0.0 {
        let br = (spec.belly.min(1.0) * rect.height).min(hy).min(hx);
        (br, ((hx - br).max(0.0) * spec.belly_w.clamp(0.0, 1.0)).max(1.0))
    } else {
        (0.0, 0.0)
    };
    let k = (spec.blend.max(0.0) * rect.height).max(1.0);
    let sheet_hy = hy - sag * 0.5;
    // Bottom (sheet_r) and top (attach) corner radii: when the pair overfills
    // the sheet height, scale both down proportionally — 0.5 + 0.5 is the
    // fully continuous egg.
    let mut sr = (spec.sheet_r.clamp(0.0, 1.0) * rect.height).min(hx);
    let mut ar = (spec.attach.clamp(0.0, 1.0) * rect.height).min(hx);
    let sheet_h = (2.0 * sheet_hy).max(0.0);
    if sr + ar > sheet_h && sr + ar > 0.0 {
        let f = sheet_h / (sr + ar);
        sr *= f;
        ar *= f;
    }
    let band = (spec.band.max(0.05) * rect.height).max(1.0);
    // Bottom-bow edge rise; the shader derives the arc radius from it per drop
    // (R = hx^2/2*rise).
    let bow = (spec.bow.clamp(0.0, 0.5) * rect.height).min(hy * 0.9);
    let sh_reach = if spec.shadow > 0.0 { (0.18 * rect.height).max(2.0) } else { 0.0 };
    DropletGeom { hx, hy, sag, br, bw, k, sr, ar, band, bow, sh_reach }
}

/// The tessellated display list's batches, scissors and rounded clips scaled
/// to physical px.
pub(crate) fn dl_batches_2d(dl_batches: &[DlBatch], scale_f32: f32) -> Vec<Batch2D> {
    dl_batches
        .iter()
        .map(|batch| Batch2D {
            scissor: batch.scissor.map(|clip| {
                (
                    (clip.x * scale_f32).max(0.0) as u32,
                    (clip.y * scale_f32).max(0.0) as u32,
                    (clip.width * scale_f32) as u32,
                    (clip.height * scale_f32) as u32,
                )
            }),
            clip_rrect: batch
                .clip_rrect
                .map(|c| [c[0] * scale_f32, c[1] * scale_f32, c[2] * scale_f32, c[3] * scale_f32, c[4] * scale_f32]),
            start: batch.start,
            end: batch.end,
            plate: batch.plate,
            blur_behind: batch.blur_behind,
        })
        .collect()
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub clip_circle: [f32; 3], // [cx, cy, r]
}

pub fn quad_vertices(x: f32, y: f32, w: f32, h: f32, sw: f32, sh: f32, c: [f32; 4]) -> [Vertex; 6] {
    let x0 = (x / sw) * 2.0 - 1.0;
    let y0 = 1.0 - (y / sh) * 2.0;
    let x1 = ((x + w) / sw) * 2.0 - 1.0;
    let y1 = 1.0 - ((y + h) / sh) * 2.0;
    [
        Vertex { position: [x0, y0], color: c, clip_circle: [0.0, 0.0, 0.0] },
        Vertex { position: [x1, y0], color: c, clip_circle: [0.0, 0.0, 0.0] },
        Vertex { position: [x0, y1], color: c, clip_circle: [0.0, 0.0, 0.0] },
        Vertex { position: [x1, y0], color: c, clip_circle: [0.0, 0.0, 0.0] },
        Vertex { position: [x1, y1], color: c, clip_circle: [0.0, 0.0, 0.0] },
        Vertex { position: [x0, y1], color: c, clip_circle: [0.0, 0.0, 0.0] },
    ]
}

pub fn quad_vertices_with_clip(
    x: f32, y: f32, w: f32, h: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
) -> [Vertex; 6] {
    let x0 = (x / sw) * 2.0 - 1.0;
    let y0 = 1.0 - (y / sh) * 2.0;
    let x1 = ((x + w) / sw) * 2.0 - 1.0;
    let y1 = 1.0 - ((y + h) / sh) * 2.0;
    [
        Vertex { position: [x0, y0], color, clip_circle },
        Vertex { position: [x1, y0], color, clip_circle },
        Vertex { position: [x0, y1], color, clip_circle },
        Vertex { position: [x1, y0], color, clip_circle },
        Vertex { position: [x1, y1], color, clip_circle },
        Vertex { position: [x0, y1], color, clip_circle },
    ]
}

/// A quad whose four corners each carry their own color, Gouraud-interpolated across both
/// triangles by the shader (`@location(0) color` has no `flat` qualifier). Corner order is
/// TL, TR, BR, BL. Keep the alpha equal on all four: negative alpha is the blur sentinel,
/// so a gradient that crossed zero would tear the triangle in half.
pub fn quad_vertices_shaded(
    x: f32, y: f32, w: f32, h: f32,
    sw: f32, sh: f32,
    c_tl: [f32; 4], c_tr: [f32; 4], c_br: [f32; 4], c_bl: [f32; 4],
    clip_circle: [f32; 3],
) -> [Vertex; 6] {
    let x0 = (x / sw) * 2.0 - 1.0;
    let y0 = 1.0 - (y / sh) * 2.0;
    let x1 = ((x + w) / sw) * 2.0 - 1.0;
    let y1 = 1.0 - ((y + h) / sh) * 2.0;
    [
        Vertex { position: [x0, y0], color: c_tl, clip_circle },
        Vertex { position: [x1, y0], color: c_tr, clip_circle },
        Vertex { position: [x0, y1], color: c_bl, clip_circle },
        Vertex { position: [x1, y0], color: c_tr, clip_circle },
        Vertex { position: [x1, y1], color: c_br, clip_circle },
        Vertex { position: [x0, y1], color: c_bl, clip_circle },
    ]
}

pub fn quad_vertices_clipped(
    x: f32, y: f32, w: f32, h: f32,
    surface_w: f32, surface_h: f32,
    color: [f32; 4],
    clip: (f32, f32, f32, f32),
    clip_circle: [f32; 3],
) -> Vec<Vertex> {
    let (cx0, cy0, cx1, cy1) = clip;
    let ix0 = x.max(cx0);
    let iy0 = y.max(cy0);
    let ix1 = (x + w).min(cx1);
    let iy1 = (y + h).min(cy1);
    if ix1 <= ix0 || iy1 <= iy0 {
        return Vec::new();
    }
    quad_vertices_with_clip(ix0, iy0, ix1 - ix0, iy1 - iy0, surface_w, surface_h, color, clip_circle).to_vec()
}

pub fn line_vertices(
    x1: f32, y1: f32, x2: f32, y2: f32,
    thickness: f32,
    sw: f32, sh: f32,
    c: [f32; 4]
) -> [Vertex; 6] {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return quad_vertices(x1 - thickness/2.0, y1 - thickness/2.0, thickness, thickness, sw, sh, c);
    }
    let ux = dx / len;
    let uy = dy / len;
    let nx = -uy;
    let ny = ux;
    
    let half_t = thickness * 0.5;
    let p0x = x1 + nx * half_t;
    let p0y = y1 + ny * half_t;
    let p1x = x1 - nx * half_t;
    let p1y = y1 - ny * half_t;
    let p2x = x2 - nx * half_t;
    let p2y = y2 - ny * half_t;
    let p3x = x2 + nx * half_t;
    let p3y = y2 + ny * half_t;

    let ndc_p0x = (p0x / sw) * 2.0 - 1.0;
    let ndc_p0y = 1.0 - (p0y / sh) * 2.0;
    let ndc_p1x = (p1x / sw) * 2.0 - 1.0;
    let ndc_p1y = 1.0 - (p1y / sh) * 2.0;
    let ndc_p2x = (p2x / sw) * 2.0 - 1.0;
    let ndc_p2y = 1.0 - (p2y / sh) * 2.0;
    let ndc_p3x = (p3x / sw) * 2.0 - 1.0;
    let ndc_p3y = 1.0 - (p3y / sh) * 2.0;

    let clip_circle = [0.0, 0.0, 0.0];
    [
        Vertex { position: [ndc_p0x, ndc_p0y], color: c, clip_circle },
        Vertex { position: [ndc_p1x, ndc_p1y], color: c, clip_circle },
        Vertex { position: [ndc_p2x, ndc_p2y], color: c, clip_circle },
        Vertex { position: [ndc_p0x, ndc_p0y], color: c, clip_circle },
        Vertex { position: [ndc_p2x, ndc_p2y], color: c, clip_circle },
        Vertex { position: [ndc_p3x, ndc_p3y], color: c, clip_circle },
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum LineCap {
    Arrow,
    Round,
    Flat,
}

pub fn vector_vertices(
    x1: f32, y1: f32, x2: f32, y2: f32,
    thickness: f32,
    sw: f32, sh: f32,
    c: [f32; 4],
    line_cap: LineCap,
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return quad_vertices(x1 - thickness/2.0, y1 - thickness/2.0, thickness, thickness, sw, sh, c).to_vec();
    }
    
    match line_cap {
        LineCap::Arrow => {
            let ux = dx / len;
            let uy = dy / len;
            let nx = -uy;
            let ny = ux;
            
            let arrow_len = (thickness * 3.0).max(10.0).min(len);
            let arrow_width = (thickness * 2.5).max(8.0);
            
            let line_x2 = x2 - ux * arrow_len;
            let line_y2 = y2 - uy * arrow_len;
            
            if len > arrow_len {
                verts.extend_from_slice(&line_vertices(x1, y1, line_x2, line_y2, thickness, sw, sh, c));
            }
            
            let bx = line_x2;
            let by = line_y2;
            
            let w1x = bx + nx * (arrow_width * 0.5);
            let w1y = by + ny * (arrow_width * 0.5);
            let w2x = bx - nx * (arrow_width * 0.5);
            let w2y = by - ny * (arrow_width * 0.5);
            
            let ndc_tip_x = (x2 / sw) * 2.0 - 1.0;
            let ndc_tip_y = 1.0 - (y2 / sh) * 2.0;
            let ndc_w1x = (w1x / sw) * 2.0 - 1.0;
            let ndc_w1y = 1.0 - (w1y / sh) * 2.0;
            let ndc_w2x = (w2x / sw) * 2.0 - 1.0;
            let ndc_w2y = 1.0 - (w2y / sh) * 2.0;
            
            let clip_circle = [0.0, 0.0, 0.0];
            verts.push(Vertex { position: [ndc_tip_x, ndc_tip_y], color: c, clip_circle });
            verts.push(Vertex { position: [ndc_w1x, ndc_w1y], color: c, clip_circle });
            verts.push(Vertex { position: [ndc_w2x, ndc_w2y], color: c, clip_circle });
        }
        LineCap::Round => {
            push_feathered_line_vertices(x1, y1, x2, y2, thickness, sw, sh, c, &mut verts);
            let clip_circle = [0.0, 0.0, 0.0];
            verts.extend(circle_vertices(x2, y2, thickness / 2.0, sw, sh, c, 16, clip_circle));
        }
        LineCap::Flat => {
            push_feathered_line_vertices(x1, y1, x2, y2, thickness, sw, sh, c, &mut verts);
        }
    }

    verts
}

/// `line_vertices` with a half-px alpha ramp along each long edge (the arc
/// tessellator's poor-man's AA) — diagonal strokes resolve smoothly instead of
/// stair-stepping. Axis-aligned strokes keep the crisp single-quad path:
/// feathering a pixel-snapped hairline would only blur it.
fn push_feathered_line_vertices(
    x1: f32, y1: f32, x2: f32, y2: f32,
    thickness: f32,
    sw: f32, sh: f32,
    c: [f32; 4],
    out: &mut Vec<Vertex>,
) {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 || dx.abs() < 0.01 || dy.abs() < 0.01 {
        out.extend_from_slice(&line_vertices(x1, y1, x2, y2, thickness, sw, sh, c));
        return;
    }
    let (nx, ny) = (-dy / len, dx / len);
    let f = 0.5f32.min(thickness * 0.25);
    let half = thickness * 0.5;
    // (offset at band start, offset at band end, alpha at start, alpha at end)
    let bands = [
        (-half - f, -half + f, 0.0, c[3]),
        (-half + f, half - f, c[3], c[3]),
        (half - f, half + f, c[3], 0.0),
    ];
    for &(oa, ob, aa, ab) in &bands {
        let ca = [c[0], c[1], c[2], aa];
        let cb = [c[0], c[1], c[2], ab];
        let p = |x: f32, y: f32, o: f32| -> [f32; 2] {
            [((x + nx * o) / sw) * 2.0 - 1.0, 1.0 - ((y + ny * o) / sh) * 2.0]
        };
        let clip_circle = [0.0, 0.0, 0.0];
        let (a1, b1) = (p(x1, y1, oa), p(x1, y1, ob));
        let (a2, b2) = (p(x2, y2, oa), p(x2, y2, ob));
        out.push(Vertex { position: a1, color: ca, clip_circle });
        out.push(Vertex { position: b1, color: cb, clip_circle });
        out.push(Vertex { position: b2, color: cb, clip_circle });
        out.push(Vertex { position: a1, color: ca, clip_circle });
        out.push(Vertex { position: b2, color: cb, clip_circle });
        out.push(Vertex { position: a2, color: ca, clip_circle });
    }
}

pub fn rounded_rect_vertices_corners(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
    corners: (bool, bool, bool, bool),
    clip_rect: Option<(f32, f32, f32, f32)>,
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    let radii = crate::widget::CornerRadii::new(
        if corners.0 { r } else { 0.0 },
        if corners.1 { r } else { 0.0 },
        if corners.2 { r } else { 0.0 },
        if corners.3 { r } else { 0.0 },
    );
    push_rounded_rect_vertices_corners(x, y, ww, h, radii, sw, sh, color, clip_circle, clip_rect, &mut verts);
    verts
}

/// Sample of the unit superellipse |x|^n + |y|^n = 1 at circle parameter θ —
/// the (cos θ, sin θ) replacement the corner fans use. Exactly the circle at
/// n = 2; higher `corner_shape` exponents give the DE's continuous-curvature
/// corners, so widget silhouettes follow the same corner family as the
/// SDF-lit plates. `e` is 2/n, hoisted by callers. Tangent points at the
/// quadrant ends are pinned to the exact axis points (see below), so fans
/// tile exactly against the body rects and edge strips.
#[inline]
fn superellipse_pt(theta: f32, e: f32) -> (f32, f32) {
    let (s, c) = theta.sin_cos();
    // f32 sin/cos are not exactly 0 at a quadrant end (sin(PI) is -8.7e-8),
    // and the fractional power magnifies that noise by orders of magnitude:
    // at corner_shape 4.5 it is 7e-4, which on the desktop grid's 138 px
    // cell corners put the fan's tangent vertex 0.1 px short of the body
    // quad's edge. The fan's edge then tilts away from the quad's, and the
    // row of pixel centres between them is covered by neither: a stray
    // gap-coloured line 32 px long at every cell's left edge, and a lone
    // gap pixel on the bottom row where the arc meets the body. Snap the
    // ends to the exact axis points so fans tile against the rects.
    let axis = |v: f32| -> f32 {
        if v.abs() < 1e-6 {
            0.0
        } else if v.abs() > 1.0 - 1e-6 {
            v.signum()
        } else {
            v.signum() * v.abs().powf(e)
        }
    };
    (axis(c), axis(s))
}

/// Feathered glow ([`Prim::Glow`]): the rounded rect's interior fills at the
/// color's alpha and concentric outline rings fade it to zero across `reach`
/// px outside the boundary. Alpha rides the VERTICES, so the GPU interpolates
/// a per-pixel-smooth falloff between rings — stacked translucent layers band
/// visibly; this cannot. Ring alphas sit on a quadratic ease-out, giving the
/// vignette profile piecewise-linearly with kinks below visibility at glow
/// alphas. Corners sample [`superellipse_pt`], so a glow's silhouette sits in
/// the same corner family as the cells, nodes, and plates it highlights.
pub fn push_glow_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    radius: f32, reach: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    if ww <= 0.0 || h <= 0.0 || color[3].abs() <= 0.0005 || sw <= 0.0 || sh <= 0.0 {
        return;
    }
    let r0 = radius.clamp(0.0, ww.min(h) * 0.5);
    let ctl = (x + r0, y + r0);
    let ctr = (x + ww - r0, y + r0);
    let cbr = (x + ww - r0, y + h - r0);
    let cbl = (x + r0, y + h - r0);
    const K: usize = 10;
    use std::f32::consts::PI;
    let corner_e = 2.0 / crate::layout::corner_shape();
    // One outline ring `off` px outside the boundary, clockwise from the
    // top-left arc; every ring shares the layout, so strips never twist.
    let ring = |off: f32| -> Vec<[f32; 2]> {
        let r = (r0 + off).max(0.0);
        let mut pts = Vec::with_capacity(4 * (K + 1));
        let corners = [
            (ctl, PI, 1.5 * PI),
            (ctr, 1.5 * PI, 2.0 * PI),
            (cbr, 0.0, 0.5 * PI),
            (cbl, 0.5 * PI, PI),
        ];
        for ((cx, cy), a0, a1) in corners {
            for k in 0..=K {
                let a = a0 + (a1 - a0) * (k as f32 / K as f32);
                let (ux, uy) = superellipse_pt(a, corner_e);
                pts.push([cx + r * ux, cy + r * uy]);
            }
        }
        pts
    };
    let to_v = |p: [f32; 2], a: f32| Vertex {
        position: [(p[0] / sw) * 2.0 - 1.0, 1.0 - (p[1] / sh) * 2.0],
        color: [color[0], color[1], color[2], a],
        clip_circle,
    };

    let rings: Vec<(Vec<[f32; 2]>, f32)> = [0.0f32, 0.35, 0.7, 1.0]
        .iter()
        .map(|&t| (ring(reach * t), color[3] * (1.0 - t) * (1.0 - t)))
        .collect();
    let n = rings[0].0.len();

    // Interior: a fan from the rect center over the innermost ring (a rounded
    // rect is convex, so the fan covers it exactly), uniform core alpha.
    let center = [x + ww * 0.5, y + h * 0.5];
    for i in 0..n {
        let p1 = rings[0].0[i];
        let p2 = rings[0].0[(i + 1) % n];
        out.push(to_v(center, color[3]));
        out.push(to_v(p1, color[3]));
        out.push(to_v(p2, color[3]));
    }
    // The feather: strips between consecutive rings, each vertex carrying its
    // ring's alpha.
    for w in rings.windows(2) {
        let (inner, ia) = (&w[0].0, w[0].1);
        let (outer, oa) = (&w[1].0, w[1].1);
        for i in 0..n {
            let a1 = inner[i];
            let a2 = inner[(i + 1) % n];
            let b1 = outer[i];
            let b2 = outer[(i + 1) % n];
            out.push(to_v(a1, ia));
            out.push(to_v(b1, oa));
            out.push(to_v(a2, ia));
            out.push(to_v(a2, ia));
            out.push(to_v(b1, oa));
            out.push(to_v(b2, oa));
        }
    }
}

pub fn push_rounded_rect_vertices_corners(
    x: f32, y: f32, ww: f32, h: f32,
    radii: crate::widget::CornerRadii,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
    clip_rect: Option<(f32, f32, f32, f32)>,
    out: &mut Vec<Vertex>,
) {
    let corner_e = 2.0 / crate::layout::corner_shape();
    let mut r_tl = radii.top_left.max(0.0);
    let mut r_tr = radii.top_right.max(0.0);
    let mut r_br = radii.bottom_right.max(0.0);
    let mut r_bl = radii.bottom_left.max(0.0);

    // Simple scale clamping
    let sum_top = r_tl + r_tr;
    if sum_top > ww {
        let f = ww / sum_top;
        r_tl *= f;
        r_tr *= f;
    }
    let sum_bottom = r_bl + r_br;
    if sum_bottom > ww {
        let f = ww / sum_bottom;
        r_bl *= f;
        r_br *= f;
    }
    let sum_left = r_tl + r_bl;
    if sum_left > h {
        let f = h / sum_left;
        r_tl *= f;
        r_bl *= f;
    }
    let sum_right = r_tr + r_br;
    if sum_right > h {
        let f = h / sum_right;
        r_tr *= f;
        r_br *= f;
    }

    let clamp_x = |val: f32| -> f32 {
        if let Some((cx0, _, cx1, _)) = clip_rect {
            val.max(cx0).min(cx1)
        } else {
            val
        }
    };
    let clamp_y = |val: f32| -> f32 {
        if let Some((_, cy0, _, cy1)) = clip_rect {
            val.max(cy0).min(cy1)
        } else {
            val
        }
    };

    let push_quad = |verts: &mut Vec<Vertex>, qx: f32, qy: f32, qw: f32, qh: f32| {
        let x0 = clamp_x(qx);
        let y0 = clamp_y(qy);
        let x1 = clamp_x(qx + qw);
        let y1 = clamp_y(qy + qh);
        
        if x1 <= x0 || y1 <= y0 {
            return;
        }

        let ndc_x0 = (x0 / sw) * 2.0 - 1.0;
        let ndc_y0 = 1.0 - (y0 / sh) * 2.0;
        let ndc_x1 = (x1 / sw) * 2.0 - 1.0;
        let ndc_y1 = 1.0 - (y1 / sh) * 2.0;
        
        verts.push(Vertex { position: [ndc_x0, ndc_y0], color, clip_circle });
        verts.push(Vertex { position: [ndc_x1, ndc_y0], color, clip_circle });
        verts.push(Vertex { position: [ndc_x0, ndc_y1], color, clip_circle });
        verts.push(Vertex { position: [ndc_x1, ndc_y0], color, clip_circle });
        verts.push(Vertex { position: [ndc_x1, ndc_y1], color, clip_circle });
        verts.push(Vertex { position: [ndc_x0, ndc_y1], color, clip_circle });
    };

    let has_corners = r_tl > 0.1 || r_tr > 0.1 || r_br > 0.1 || r_bl > 0.1;
    if !has_corners {
        push_quad(out, x, y, ww, h);
        return;
    }

    // Body rectangles
    let mid_x0 = r_tl.max(r_bl);
    let mid_x1 = ww - r_tr.max(r_br);
    if mid_x1 > mid_x0 {
        push_quad(out, x + mid_x0, y, mid_x1 - mid_x0, h);
    }
    if h > r_tl + r_bl {
        push_quad(out, x, y + r_tl, mid_x0, h - r_tl - r_bl);
    }
    if h > r_tr + r_br {
        push_quad(out, x + mid_x1, y + r_tr, ww - mid_x1, h - r_tr - r_br);
    }

    // Corner rendering. The fans are FEATHERED: the fan body stops half a
    // pixel short of the silhouette and a strip fades from opaque at
    // silhouette-0.5 to transparent at silhouette+0.5, so the arc
    // anti-aliases instead of rasterizing a hard staircase — invisible on
    // HiDPI widget buffers, glaring on the desktop grid's world-scale
    // cells. Perceived size is unchanged (the 50%-coverage line stays on
    // the exact silhouette). Radii too small to feather keep the hard fan.
    let segments = 16;
    let fade = [color[0], color[1], color[2], 0.0];
    let to_ndc = |px: f32, py: f32| -> [f32; 2] {
        [(px / sw) * 2.0 - 1.0, 1.0 - (py / sh) * 2.0]
    };
    let push_corner = |out: &mut Vec<Vertex>, cx: f32, cy: f32, r: f32, start: f32, end: f32| {
        let feather = r > 1.5;
        let r_fan = if feather { r - 0.5 } else { r };
        let r_out = r + 0.5;
        for i in 0..segments {
            let theta1 = start + (i as f32) * (end - start) / (segments as f32);
            let theta2 = start + ((i + 1) as f32) * (end - start) / (segments as f32);

            let (c1, s1) = superellipse_pt(theta1, corner_e);
            let (c2, s2) = superellipse_pt(theta2, corner_e);
            let p0 = to_ndc(clamp_x(cx), clamp_y(cy));
            let p1 = to_ndc(clamp_x(cx + r_fan * c1), clamp_y(cy + r_fan * s1));
            let p2 = to_ndc(clamp_x(cx + r_fan * c2), clamp_y(cy + r_fan * s2));

            out.push(Vertex { position: p0, color, clip_circle });
            out.push(Vertex { position: p1, color, clip_circle });
            out.push(Vertex { position: p2, color, clip_circle });

            if feather {
                let q1 = to_ndc(clamp_x(cx + r_out * c1), clamp_y(cy + r_out * s1));
                let q2 = to_ndc(clamp_x(cx + r_out * c2), clamp_y(cy + r_out * s2));
                out.push(Vertex { position: p1, color, clip_circle });
                out.push(Vertex { position: q1, color: fade, clip_circle });
                out.push(Vertex { position: q2, color: fade, clip_circle });
                out.push(Vertex { position: p1, color, clip_circle });
                out.push(Vertex { position: q2, color: fade, clip_circle });
                out.push(Vertex { position: p2, color, clip_circle });
            }
        }
    };

    // Top-Left
    if r_tl > 0.1 {
        push_corner(out, x + r_tl, y + r_tl, r_tl, std::f32::consts::PI, 1.5 * std::f32::consts::PI);
        if mid_x0 > r_tl {
            push_quad(out, x + r_tl, y, mid_x0 - r_tl, r_tl);
        }
    }

    // Top-Right
    if r_tr > 0.1 {
        push_corner(out, x + ww - r_tr, y + r_tr, r_tr, 1.5 * std::f32::consts::PI, 2.0 * std::f32::consts::PI);
        if ww - mid_x1 > r_tr {
            push_quad(out, x + mid_x1, y, ww - mid_x1 - r_tr, r_tr);
        }
    }

    // Bottom-Right
    if r_br > 0.1 {
        push_corner(out, x + ww - r_br, y + h - r_br, r_br, 0.0, 0.5 * std::f32::consts::PI);
        if ww - mid_x1 > r_br {
            push_quad(out, x + mid_x1, y + h - r_br, ww - mid_x1 - r_br, r_br);
        }
    }

    // Bottom-Left
    if r_bl > 0.1 {
        push_corner(out, x + r_bl, y + h - r_bl, r_bl, 0.5 * std::f32::consts::PI, std::f32::consts::PI);
        if mid_x0 > r_bl {
            push_quad(out, x + r_bl, y + h - r_bl, mid_x0 - r_bl, r_bl);
        }
    }
}

pub fn rounded_rect_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    push_rounded_rect_vertices_corners(x, y, ww, h, crate::widget::CornerRadii::uniform(r), sw, sh, color, clip_circle, None, &mut verts);
    verts
}

pub fn push_rounded_rect_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    push_rounded_rect_vertices_corners(x, y, ww, h, crate::widget::CornerRadii::uniform(r), sw, sh, color, clip_circle, None, out);
}

pub fn plate_bevel_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    t: f32,
    sw: f32, sh: f32,
    base_color: [f32; 4],
    clip_circle: [f32; 3],
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    push_plate_bevel_vertices(x, y, ww, h, r, t, sw, sh, base_color, clip_circle, &mut verts);
    verts
}

pub fn push_plate_bevel_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    t: f32,
    sw: f32, sh: f32,
    base_color: [f32; 4],
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    push_bevel_edge_vertices(x, y, ww, h, r, t, sw, sh, base_color, clip_circle, 1.0, out);
}

/// The bevel edge shading, with the light direction selectable: `light_sign` is `1.0`
/// for a raised plate (edges facing `light_source_position` are lit) and `-1.0` for a
/// recess (those same edges fall into shadow instead, and the far edges catch the
/// light). Negating the whole light vector flips every edge and every corner segment
/// consistently, because both the flat-edge factors and the arc-normal dot product
/// below are linear in it.
pub fn push_bevel_edge_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    t: f32,
    sw: f32, sh: f32,
    base_color: [f32; 4],
    clip_circle: [f32; 3],
    light_sign: f32,
    out: &mut Vec<Vertex>,
) {
    push_bevel_edge_vertices_radii(
        x, y, ww, h, (r, r, r, r), t, sw, sh, base_color, clip_circle, light_sign, out,
    );
}

/// As [`push_bevel_edge_vertices`], but with a per-corner radius (TL, TR, BR, BL) so the
/// lip can follow a shape whose corners differ — a recess carved along the top of a
/// rounded plate needs the plate's radius on its top corners and square ones where it
/// meets the content below. A uniform radius there would either square off the plate's
/// arc (painting a notch outside it) or wrongly round the inner corners.
pub fn push_bevel_edge_vertices_radii(
    x: f32, y: f32, ww: f32, h: f32,
    radii: (f32, f32, f32, f32),
    t: f32,
    sw: f32, sh: f32,
    base_color: [f32; 4],
    clip_circle: [f32; 3],
    light_sign: f32,
    out: &mut Vec<Vertex>,
) {
    push_bevel_edge_vertices_banded(
        x, y, ww, h, radii, t, sw, sh, base_color, clip_circle, light_sign,
        default_bevel_bands(t), (true, true, true, true), EdgeKind::Rim, out,
    );
}

/// What kind of height change an edge represents. The two shade differently because they
/// are different shapes, and using one where the other belongs is what makes a bevel read
/// as a drawn line instead of a surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    /// The surface *ends* here: a quarter-round rolling from face-on at the inner edge of
    /// the lip to fully in-plane at the outer boundary, where it drops away. The shading
    /// therefore peaks exactly at the boundary and dies inward. This is a plate's outer
    /// perimeter.
    Rim,
    /// The surface *continues* at a different height: one plateau steps down to another.
    /// A height field that falls monotonically across the transition has its normal tilted
    /// toward the low side the whole way, steepest in the middle and flat at both ends —
    /// so the shading is a bump straddling the boundary, not a band butted against it.
    /// Hanging the band on one side instead leaves the seam the eye reads as a drawn line.
    Step,
}

/// Shading across an edge at signed distance `d` from the boundary (positive = toward the
/// shape's interior), for a transition of width `t`. Returns the light term as a fraction
/// of full tilt.
#[inline]
fn bevel_profile(kind: EdgeKind, d: f32, t: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    match kind {
        // Normal rotates from in-plane (d = 0) to face-on (d = t): sine of what tilt is
        // left. A linear ramp here reads as a flat 45° chamfer instead of a roll.
        EdgeKind::Rim => ((1.0 - (d / t).clamp(0.0, 1.0)) * std::f32::consts::FRAC_PI_2).sin(),
        // Symmetric bump over [-t/2, +t/2], zero at both ends so the transition blends into
        // both plateaus with no seam.
        EdgeKind::Step => {
            let s = (d / t + 0.5).clamp(0.0, 1.0);
            (s * std::f32::consts::PI).sin()
        }
    }
}

/// The light-independent curvature term at signed distance `d` — the second depth cue,
/// on top of the directional one. Curvature shading is what ambient light does: convex
/// surface catches it from everywhere (bright), concave is self-occluded (dark). Because
/// it does not rotate with the light, it survives exactly where the directional term
/// dies — walls parallel to the light vector — so no edge ever vanishes entirely.
///
/// `high_sign` is +1 when the rect interior is the HIGH side of the transition and -1
/// when it is the low side (a recess). Geometry, not lighting: it does not flip with
/// `light_sign`... except that for these 2.5D shapes the two are the same number, since
/// a raised shape is lit like a plateau and shaded like one.
#[inline]
fn bevel_curvature(kind: EdgeKind, d: f32, t: f32, high_sign: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    match kind {
        // A rim is convex everywhere, tightest right at the silhouette: a bright crest
        // line hugging the boundary and dying fast inward. This is the line that makes
        // glass read as glass — the edge catches ambient light all the way around, even
        // (dimmer, via the gain asymmetry below) on the side facing away from the light.
        EdgeKind::Rim => {
            let u = (d / t).clamp(0.0, 1.0);
            let f = 1.0 - u;
            CREST_RATIO * f * f * f
        }
        // An S-curve step is convex on its high half (the shoulder) and concave on its
        // low half (the fillet, where the wall meets the floor): antisymmetric, zero at
        // the ends (no seam against either plateau) and at the midpoint.
        EdgeKind::Step => {
            let s = (d / t + 0.5).clamp(0.0, 1.0);
            let outer_is_high = -high_sign; // d < 0 is outside the rect
            // sin(2πs) is positive on the outer half — the shoulder when the outside is
            // the high side — and negative on the inner (fillet) half.
            AO_RATIO * outer_is_high * (s * std::f32::consts::TAU).sin()
        }
    }
}

/// Crest amplitude as a fraction of `bevel_depth` — how much brighter a rim's silhouette
/// line is than flat surface under even light. Must stay clearly below ~0.7 (the
/// projection of a 135° light onto an axis edge), or it cancels the directional shadow
/// on the dark side and the rim goes flat there instead of showing a faint bright line
/// over a shadowed roll.
const CREST_RATIO: f32 = 0.4;
/// Shoulder/fillet amplitude as a fraction of `bevel_depth`.
const AO_RATIO: f32 = 0.6;
/// Per-sign overlay gains. These are asymmetric the opposite way from intuition: on the
/// dark bases this DE runs, white-over blending (`b + a(1-b)`) moves the pixel far more
/// per unit alpha than black-over (`b(1-a)`) — a dark surface has little brightness for
/// black to take away. The old subtractive shading effectively crushed shadow sides to
/// black in linear space; the black overlay needs a high gain to keep shadows reading
/// at all, while white needs damping to keep highlights from blowing out.
const LIGHT_GAIN: f32 = 0.7;
const DARK_GAIN: f32 = 3.0;

/// A shading value (already scaled by `bevel_depth`) as the two overlay passes: the lit
/// pass is translucent white, the shadow pass translucent black. Painting the
/// *modulation* instead of a resolved surface color is what lets relief primitives compose — a step
/// crossing a rim shades the rim's gradient instead of stamping a flat band over it, a
/// lip on a translucent plate no longer doubles its opacity, and a recess needs no
/// knowledge of the surface color it carves.
///
/// Why two passes with fixed RGB rather than one signed color: a primitive whose value
/// crosses zero inside a band would interpolate white→black through mid-gray at
/// non-negligible alpha — on a dark base a *brightening* artifact right where the
/// shading should vanish. With per-pass alphas clamped at the crossing, each pass fades
/// to zero there and the hue can never be wrong. Alphas also stay non-negative on every
/// vertex, which the renderer requires (negative alpha is the blur sentinel).
#[inline]
fn overlay_light(v: f32) -> [f32; 4] {
    [1.0, 1.0, 1.0, (v.max(0.0) * LIGHT_GAIN).min(1.0)]
}
#[inline]
fn overlay_dark(v: f32) -> [f32; 4] {
    [0.0, 0.0, 0.0, ((-v).max(0.0) * DARK_GAIN).min(1.0)]
}

/// The signed distance range an edge's shading occupies, relative to the boundary.
#[inline]
fn bevel_span(kind: EdgeKind, t: f32) -> (f32, f32) {
    match kind {
        EdgeKind::Rim => (0.0, t),
        EdgeKind::Step => (-0.5 * t, 0.5 * t),
    }
}

/// How many gradient bands to slice a lip of thickness `t` into. Vertex colors interpolate
/// linearly, so each band is a chord of the shading curve; one band per ~1.25px keeps the
/// error under a shade step without emitting geometry finer than the display resolves.
/// The cap rose with the curvature term: a step now has two features across its width
/// (shoulder and fillet), so it needs double the samples a single bump did.
fn default_bevel_bands(t: f32) -> usize {
    ((t / 1.25).ceil() as usize).clamp(1, 12)
}

/// As [`push_bevel_edge_vertices_radii`], with the band count forced and the walls
/// selectable — for callers that want a coarser or finer roll-off than thickness alone
/// implies, or that are shading a step rather than a closed shape.
///
/// `edges` is (top, right, bottom, left). Suppressing a wall matters for a region that
/// runs flush to the surface's own edge: a full-width menubar sunk into the top of a plate
/// is a *plateau one step down*, not a trough, so its only real wall is the one facing the
/// content. Drawing the other three would carve a lip along the plate's outer edge, where
/// the plate's own roll already lives, and the two would fight.
pub fn push_bevel_edge_vertices_banded(
    x: f32, y: f32, ww: f32, h: f32,
    radii: (f32, f32, f32, f32),
    t: f32,
    sw: f32, sh: f32,
    base_color: [f32; 4],
    clip_circle: [f32; 3],
    light_sign: f32,
    bands: usize,
    edges: (bool, bool, bool, bool),
    kind: EdgeKind,
    out: &mut Vec<Vertex>,
) {
    // Floored for the same reason as `plate_push_raised`'s cap: a negative
    // extent must degrade to no ring, not panic in `clamp`.
    let cap = (ww.min(h) * 0.5).max(0.0);
    let (tl, tr, br, bl) = (
        radii.0.clamp(0.0, cap),
        radii.1.clamp(0.0, cap),
        radii.2.clamp(0.0, cap),
        radii.3.clamp(0.0, cap),
    );
    let t = t.clamp(0.0, cap);
    if t <= 0.0 {
        return;
    }
    let bands = bands.max(1);

    let rad = crate::layout::light_source_position();
    let lx = rad.cos() * light_sign;
    let ly = -rad.sin() * light_sign;
    let depth = crate::layout::bevel_depth();

    // `base_color` is no longer painted: shading is an overlay (see `overlay_color`), so
    // the surface below shows through with its own gradients and translucency intact.
    let _ = base_color;
    // Shading (directional + curvature, scaled by bevel_depth) at signed distance `d`,
    // for an edge whose outward flat normal is `dir`. A `Step` band runs negative — it
    // straddles the boundary into the plateau outside the rect, which is exactly what
    // removes the seam.
    let value = |dot: f32, d: f32| {
        depth * (bevel_profile(kind, d, t) * dot + bevel_curvature(kind, d, t, light_sign))
    };
    // The (up to two) overlay color pairs for a band running from value `v0` to `v1`:
    // one white pair and/or one black pair, each pass fading to zero alpha wherever the
    // value has the other sign. Both fire only when the band straddles the terminator.
    let passes = |v0: f32, v1: f32| -> [Option<([f32; 4], [f32; 4])>; 2] {
        [
            (v0 > 0.0 || v1 > 0.0).then(|| (overlay_light(v0), overlay_light(v1))),
            (v0 < 0.0 || v1 < 0.0).then(|| (overlay_dark(v0), overlay_dark(v1))),
        ]
    };
    let (span_lo, span_hi) = bevel_span(kind, t);

    // Each flat edge spans between its two adjoining corner radii, not a single uniform
    // inset — that is what lets the corners differ. At a square corner there is no arc to
    // cover the t×t patch where two edges meet, so the horizontal edges claim it (they run
    // the full span) and the vertical ones inset by `t`; overlapping them instead would
    // double-blend that patch, which shows as a dark notch on a translucent surface.
    let (left_top, left_bot) = (if tl > 0.0 { tl } else { t }, if bl > 0.0 { bl } else { t });
    let (right_top, right_bot) = (if tr > 0.0 { tr } else { t }, if br > 0.0 { br } else { t });
    let top_w = ww - tl - tr;
    let bottom_w = ww - bl - br;
    let left_h = h - left_top - left_bot;
    let right_h = h - right_top - right_bot;

    for k in 0..bands {
        let d0 = span_lo + (span_hi - span_lo) * (k as f32 / bands as f32);
        let d1 = span_lo + (span_hi - span_lo) * ((k + 1) as f32 / bands as f32);
        let bw = d1 - d0;

        // Top: outward normal (0,-1); the gradient runs downward, into the surface.
        if top_w > 0.0 && edges.0 {
            let (v0, v1) = (value(-ly, d0), value(-ly, d1));
            for (c0, c1) in passes(v0, v1).into_iter().flatten() {
                out.extend_from_slice(&quad_vertices_shaded(
                    x + tl, y + d0, top_w, bw, sw, sh, c0, c0, c1, c1, clip_circle,
                ));
            }
        }
        // Bottom: outward normal (0,1); gradient runs upward.
        if bottom_w > 0.0 && edges.2 {
            let (v0, v1) = (value(ly, d0), value(ly, d1));
            for (c0, c1) in passes(v0, v1).into_iter().flatten() {
                out.extend_from_slice(&quad_vertices_shaded(
                    x + bl, y + h - d1, bottom_w, bw, sw, sh, c1, c1, c0, c0, clip_circle,
                ));
            }
        }
        // Left: outward normal (-1,0); gradient runs rightward.
        if left_h > 0.0 && edges.3 {
            let (v0, v1) = (value(-lx, d0), value(-lx, d1));
            for (c0, c1) in passes(v0, v1).into_iter().flatten() {
                out.extend_from_slice(&quad_vertices_shaded(
                    x + d0, y + left_top, bw, left_h, sw, sh, c0, c1, c1, c0, clip_circle,
                ));
            }
        }
        // Right: outward normal (1,0); gradient runs leftward.
        if right_h > 0.0 && edges.1 {
            let (v0, v1) = (value(lx, d0), value(lx, d1));
            for (c0, c1) in passes(v0, v1).into_iter().flatten() {
                out.extend_from_slice(&quad_vertices_shaded(
                    x + ww - d1, y + right_top, bw, right_h, sw, sh, c1, c0, c0, c1, clip_circle,
                ));
            }
        }
    }

    // A corner arc belongs to both of its adjoining walls, so it is drawn only when both
    // are — otherwise a suppressed wall would still get a quarter of a lip.
    let corners = [
        (x + tl, y + tl, tl, std::f32::consts::PI, 1.5 * std::f32::consts::PI, edges.0 && edges.3), // Top-Left
        (x + ww - tr, y + tr, tr, 1.5 * std::f32::consts::PI, 2.0 * std::f32::consts::PI, edges.0 && edges.1), // Top-Right
        (x + ww - br, y + h - br, br, 0.0, 0.5 * std::f32::consts::PI, edges.2 && edges.1), // Bottom-Right
        (x + bl, y + h - bl, bl, 0.5 * std::f32::consts::PI, std::f32::consts::PI, edges.2 && edges.3), // Bottom-Left
    ];

    for &(cx, cy, r, start_angle, end_angle, enabled) in &corners {
        // A square corner has no arc to sweep — the flat edges already met there.
        if r <= 0.0 || !enabled {
            continue;
        }
        // The corner is a quarter of a torus: shading varies along the sweep (the normal
        // swings through 90° of the light) *and* across the lip (the roll-off). Both come
        // out of the vertex colors, so one quad per (segment × band) cell is enough — no
        // faceting, unlike the 16 flat wedges this replaced.
        let segments = ((r * 0.75) as usize).clamp(8, 48);
        let ct = t.min(r);
        for j in 0..segments {
            let theta0 = start_angle + (j as f32) * (end_angle - start_angle) / (segments as f32);
            let theta1 = start_angle + ((j + 1) as f32) * (end_angle - start_angle) / (segments as f32);
            let (cos0, sin0) = (theta0.cos(), theta0.sin());
            let (cos1, sin1) = (theta1.cos(), theta1.sin());
            for k in 0..bands {
                let d0 = span_lo + (span_hi - span_lo) * (k as f32 / bands as f32);
                let d1 = span_lo + (span_hi - span_lo) * ((k + 1) as f32 / bands as f32);
                // Inward along the corner's radius is the same signed distance as inward
                // from a flat edge, so the arc scales the span the same way.
                let (r0, r1) = (r - ct * (d0 / t), r - ct * (d1 / t));
                let p = |rho: f32, c: f32, s: f32| -> [f32; 2] {
                    [
                        ((cx + rho * c) / sw) * 2.0 - 1.0,
                        1.0 - ((cy + rho * s) / sh) * 2.0,
                    ]
                };
                // Outer/inner × the two sweep ends; each vertex gets its own value, and
                // the cell is drawn once per overlay pass that has any coverage.
                let vals = [
                    value(cos0 * lx + sin0 * ly, d0),
                    value(cos1 * lx + sin1 * ly, d0),
                    value(cos1 * lx + sin1 * ly, d1),
                    value(cos0 * lx + sin0 * ly, d1),
                ];
                let geo = [
                    p(r0, cos0, sin0),
                    p(r0, cos1, sin1),
                    p(r1, cos1, sin1),
                    p(r1, cos0, sin0),
                ];
                let mut cells: [Option<fn(f32) -> [f32; 4]>; 2] = [None, None];
                if vals.iter().any(|&v| v > 0.0) {
                    cells[0] = Some(overlay_light);
                }
                if vals.iter().any(|&v| v < 0.0) {
                    cells[1] = Some(overlay_dark);
                }
                for f in cells.into_iter().flatten() {
                    let c: Vec<Vertex> = (0..4)
                        .map(|i| Vertex { position: geo[i], color: f(vals[i]), clip_circle })
                        .collect();
                    out.extend_from_slice(&[c[0], c[1], c[2], c[0], c[2], c[3]]);
                }
            }
        }
    }
}

/// How strong the face gradient is, as a fraction of `bevel_depth` at the corner nearest
/// the light. Deliberately well below the edge amplitude: the face is a plane, not a
/// roll — it only *leans* toward the light.
const FACE_RATIO: f32 = 0.35;

/// The face lighting of a plate: a single diagonal luminance gradient across the whole
/// surface, brightest at the corner facing `light_source_position` and darkest at the
/// opposite one. This is the difference between an object and a sticker: a real surface
/// under directional light is never uniform, and a perfectly flat fill makes the eye
/// read the (much smaller) edge shading as frame decoration rather than shape.
///
/// Emitted as the same two-pass white/black overlays as the relief primitives (see
/// [`overlay_light`]/[`overlay_dark`]): fixed RGB per pass, per-corner alphas clamped at
/// the terminator, bilinear across the quad. The quad is square — its corners poke past
/// a rounded plate's arcs — but the compositor clips the window surface to the same
/// radius, so the overhang never reaches the screen.
pub fn push_plate_face_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    sw: f32, sh: f32,
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    let rad = crate::layout::light_source_position();
    let (lx, ly) = (rad.cos(), -rad.sin());
    let amp = crate::layout::bevel_depth() * FACE_RATIO;
    // Corner value = how much its outward diagonal faces the light.
    let inv = std::f32::consts::FRAC_1_SQRT_2;
    let v_tl = amp * inv * (-lx - ly);
    let v_tr = amp * inv * (lx - ly);
    let v_br = amp * inv * (lx + ly);
    let v_bl = amp * inv * (-lx + ly);
    let vs = [v_tl, v_tr, v_br, v_bl];
    if vs.iter().any(|&v| v > 0.0) {
        out.extend_from_slice(&quad_vertices_shaded(
            x, y, ww, h, sw, sh,
            overlay_light(v_tl), overlay_light(v_tr), overlay_light(v_br), overlay_light(v_bl),
            clip_circle,
        ));
    }
    if vs.iter().any(|&v| v < 0.0) {
        out.extend_from_slice(&quad_vertices_shaded(
            x, y, ww, h, sw, sh,
            overlay_dark(v_tl), overlay_dark(v_tr), overlay_dark(v_br), overlay_dark(v_bl),
            clip_circle,
        ));
    }
}

pub fn push_plate_solid_border_vertices(
    x: f32, y: f32, ww: f32, h: f32,
    radii: crate::widget::CornerRadii,
    t: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    let mut r_tl = radii.top_left.max(0.0);
    let mut r_tr = radii.top_right.max(0.0);
    let mut r_br = radii.bottom_right.max(0.0);
    let mut r_bl = radii.bottom_left.max(0.0);

    // Simple scale clamping
    let sum_top = r_tl + r_tr;
    if sum_top > ww {
        let f = ww / sum_top;
        r_tl *= f;
        r_tr *= f;
    }
    let sum_bottom = r_bl + r_br;
    if sum_bottom > ww {
        let f = ww / sum_bottom;
        r_bl *= f;
        r_br *= f;
    }
    let sum_left = r_tl + r_bl;
    if sum_left > h {
        let f = h / sum_left;
        r_tl *= f;
        r_bl *= f;
    }
    let sum_right = r_tr + r_br;
    if sum_right > h {
        let f = h / sum_right;
        r_tr *= f;
        r_br *= f;
    }

    out.extend_from_slice(&quad_vertices_with_clip(x + r_tl, y, ww - r_tl - r_tr, t, sw, sh, color, clip_circle));
    out.extend_from_slice(&quad_vertices_with_clip(x, y + r_tl, t, h - r_tl - r_bl, sw, sh, color, clip_circle));
    out.extend_from_slice(&quad_vertices_with_clip(x + r_bl, y + h - t, ww - r_bl - r_br, t, sw, sh, color, clip_circle));
    out.extend_from_slice(&quad_vertices_with_clip(x + ww - t, y + r_tr, t, h - r_tr - r_br, sw, sh, color, clip_circle));

    let segments = 16;
    let corner_e = 2.0 / crate::layout::corner_shape();

    // Corner strokes as annulus strips between the outer superellipse (radius
    // r) and its inner scaled copy (r - t): at 1px thickness the scaled inner
    // curve is indistinguishable from the true parallel curve, and at
    // corner_shape 2 this is exactly the circular arc annulus. NOT
    // push_arc_background_vertices — that stays circular for genuine arcs.
    //
    // Both edges of the annulus are FEATHERED, like the fill fan's corners
    // (push_rounded_rect_vertices_corners) and push_feathered_line_vertices:
    // each fades over `f` either side of its true curve, so the 50%-coverage
    // lines stay on the exact silhouette and the stroke reads at the same
    // weight, but the arc anti-aliases instead of rasterizing a staircase
    // beside the SDF-smooth face it outlines. The straight edges stay crisp
    // quads (a pixel-snapped hairline would only blur). A corner too tight to
    // fit the inner fade keeps the hard annulus.
    let f = 0.5f32.min(t * 0.25);
    let fade = [color[0], color[1], color[2], 0.0];
    let corner = |cx: f32, cy: f32, r: f32, start: f32, end: f32, out: &mut Vec<Vertex>| {
        let r_in = (r - t).max(0.0);
        // (inner radius, outer radius, inner alpha colour, outer alpha colour)
        let bands: &[(f32, f32, [f32; 4], [f32; 4])] = if r_in - f > 0.0 {
            &[
                (r_in - f, r_in + f, fade, color),
                (r_in + f, r - f, color, color),
                (r - f, r + f, color, fade),
            ]
        } else {
            &[(r_in, r, color, color)]
        };
        let ndc = |px: f32, py: f32| [(px / sw) * 2.0 - 1.0, 1.0 - (py / sh) * 2.0];
        for i in 0..segments {
            let t1 = start + (i as f32) * (end - start) / segments as f32;
            let t2 = start + ((i + 1) as f32) * (end - start) / segments as f32;
            let (c1, s1) = superellipse_pt(t1, corner_e);
            let (c2, s2) = superellipse_pt(t2, corner_e);
            for &(ra, rb, ca, cb) in bands {
                if rb - ra <= 0.0 {
                    continue;
                }
                let o1 = ndc(cx + rb * c1, cy + rb * s1);
                let o2 = ndc(cx + rb * c2, cy + rb * s2);
                let i1 = ndc(cx + ra * c1, cy + ra * s1);
                let i2 = ndc(cx + ra * c2, cy + ra * s2);
                out.push(Vertex { position: o1, color: cb, clip_circle });
                out.push(Vertex { position: o2, color: cb, clip_circle });
                out.push(Vertex { position: i1, color: ca, clip_circle });
                out.push(Vertex { position: o2, color: cb, clip_circle });
                out.push(Vertex { position: i2, color: ca, clip_circle });
                out.push(Vertex { position: i1, color: ca, clip_circle });
            }
        }
    };

    if r_tl > 0.1 {
        corner(x + r_tl, y + r_tl, r_tl, std::f32::consts::PI, 1.5 * std::f32::consts::PI, out);
    }
    if r_tr > 0.1 {
        corner(x + ww - r_tr, y + r_tr, r_tr, 1.5 * std::f32::consts::PI, 2.0 * std::f32::consts::PI, out);
    }
    if r_br > 0.1 {
        corner(x + ww - r_br, y + h - r_br, r_br, 0.0, 0.5 * std::f32::consts::PI, out);
    }
    if r_bl > 0.1 {
        corner(x + r_bl, y + h - r_bl, r_bl, 0.5 * std::f32::consts::PI, std::f32::consts::PI, out);
    }
}

pub fn push_plate_solid_border_vertices_legacy(
    x: f32, y: f32, ww: f32, h: f32,
    r: f32,
    t: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    let radii = crate::widget::CornerRadii::uniform(r);
    push_plate_solid_border_vertices(x, y, ww, h, radii, t, sw, sh, color, clip_circle, out);
}

pub fn widget_vertices(w: &dyn crate::widget::WidgetHost, sw: f32, sh: f32, clip_circle: [f32; 3]) -> Vec<Vertex> {
    let mut verts = Vec::new();
    push_widget_vertices(w, sw, sh, clip_circle, &mut verts);
    verts
}

pub fn push_widget_vertices(w: &dyn crate::widget::WidgetHost, sw: f32, sh: f32, clip_circle: [f32; 3], out: &mut Vec<Vertex>) {
    let (x, y, ww, h) = w.rect();
    let radii = w.corner_radii();
    if let Some(thickness) = w.plate_bevel() {
        let t = thickness;
        // Full-size fill: the bevel lip is a shading overlay now, not a paint of the
        // outer ring, so an inset fill would leave the ring unfilled.
        push_rounded_rect_vertices_corners(x, y, ww, h, radii, sw, sh, w.color(), clip_circle, None, out);
        push_plate_bevel_vertices(x, y, ww, h, radii.top_left, t, sw, sh, w.color(), clip_circle, out);
    } else {
        push_rounded_rect_vertices_corners(x, y, ww, h, radii, sw, sh, w.color(), clip_circle, None, out);
        if let Some((color, thickness)) = w.solid_border() {
            push_plate_solid_border_vertices(x, y, ww, h, radii, thickness, sw, sh, color, clip_circle, out);
        }
    }

    for (cx, cy, r, t, start, end, qc) in w.extra_arcs() {
        push_arc_background_vertices(cx, cy, r, t, start, end, sw, sh, qc, 16, clip_circle, out);
    }
}

/// A contiguous run of vertices sharing one scissor rect (Phase 3 single paint path) and one
/// rounded-rect clip. `scissor` is a logical-pixel clip (`None` = unclipped); `clip_rrect` is
/// the paint walk's `[cx, cy, bx, by, r]` rounded clip in logical px (`None` = unclipped),
/// applied as per-draw push-constant state; `start..end` indexes the flat vertex buffer.
pub struct DlBatch {
    pub scissor: Option<crate::scene::layout::Rect>,
    pub clip_rrect: Option<[f32; 5]>,
    pub start: u32,
    pub end: u32,
    /// When set, this batch is one SDF-lit plate cover quad (see
    /// [`crate::draw::PlatePush`]; already in physical px). Never merged.
    pub plate: Option<crate::draw::PlatePush>,
    /// A blur-behind plate (negative-alpha color): before drawing this batch
    /// the renderer snapshots the swapchain-so-far into its snapshot image, so
    /// the blur samples everything painted beneath the plate — not just the 3D
    /// scene backdrop. Never merged.
    pub blur_behind: bool,
}

/// An image draw from the display list: `at` is the vertex index it sorts
/// before (its position in the tessellated stream); `clip` is the item's
/// paint-walk clip. Logical coordinates throughout.
pub struct DlImage {
    pub image: u32,
    pub rect: crate::scene::layout::Rect,
    pub alpha: f32,
    pub at: u32,
    pub clip: Option<crate::scene::layout::Rect>,
}

/// Tessellate a `scene::paint::DisplayList`'s geometry into a flat vertex buffer plus per-clip draw
/// batches, reusing the same tessellators as the legacy path so vertices are identical. `Text`
/// prims are skipped here — text is still rendered via the app's `text_areas()` path. `sw`/`sh` are
/// logical surface dimensions (as everywhere else); `scale` is the HiDPI factor, needed because an
/// item's circular clip rides the vertices in PHYSICAL pixels. Consecutive prims sharing a clip are
/// merged into one batch (the circle clip is per-vertex, so it never splits batches).
/// `CCE_PLATE_DEBUG=1` — trace which carves group into their host plate as exact
/// CSG features and which fall back to the standalone overlay shading.
///
/// The two paths do NOT look the same: a grouped carve is part of the plate's
/// single height field, so its wall meets the plate's rolled perimeter as a real
/// junction, while the fallback approximates that with the host-box fade. Six
/// conditions decide it, three of them dynamic (draw order, neighbouring plates,
/// whether another carve already claimed the host's feature run), so the SAME
/// widget can render either way depending on what is around it — and it does so
/// silently. That has already shipped as a bug once: a hovered button's opaque
/// fill used to sever every later button from the root plate they carve into,
/// which is why `plate_stack` is a stack (see its comment below).
///
/// Off by default and read once; the classification below runs only when set.
/// Prim discriminant name, for `CCE_PLATE_DEBUG` reporting only.
fn prim_kind(p: &crate::scene::paint::Prim) -> &'static str {
    use crate::scene::paint::Prim as P;
    match p {
        P::Quad { .. } => "Quad", P::RoundedRect { .. } => "RoundedRect",
        P::Border { .. } => "Border", P::Bevel { .. } => "Bevel",
        P::Recess { .. } => "Recess", P::Boss { .. } => "Boss",
        P::Ridge { .. } => "Ridge", P::Trough { .. } => "Trough", P::Field { .. } => "Field", P::Plate { .. } => "Plate",
        P::Arc { .. } => "Arc", P::ArcShaded { .. } => "ArcShaded",
        P::Vector { .. } => "Vector", P::Circle { .. } => "Circle",
        P::Sphere { .. } => "Sphere", P::Droplet { .. } => "Droplet",
        P::DropletScrim { .. } => "DropletScrim", P::Frame { .. } => "Frame",
        P::ConcaveFillet { .. } => "ConcaveFillet",
        P::Groove { .. } => "Groove", P::Lattice { .. } => "Lattice", P::Grout { .. } => "Grout", P::Fill { .. } => "Fill",
        P::CarveUnion { .. } => "CarveUnion", P::Glow { .. } => "Glow",
        P::Text { .. } => "Text", P::Image { .. } => "Image",
    }
}

fn plate_debug() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("CCE_PLATE_DEBUG").is_ok_and(|v| v != "0"))
}

/// Debug builds make one kind of fallback LOUD without `CCE_PLATE_DEBUG`: a
/// carve that could group (full ring, untinted) failing to while a still-open
/// plate encloses it and the carve's shaded region reaches that plate's
/// perimeter roll. There the grouped and overlay paths shade the junction
/// differently, and the rejection is one of the dynamic rules — so the SAME
/// widget can flip looks frame to frame with nothing on stderr. Not an
/// assert/panic: every rejection is conservative-CORRECT (the audit that
/// shipped CCE_PLATE_DEBUG found no misgrouping; a later plate overlapping the
/// carve genuinely must be shaded over, not under) — it is the frame-to-frame
/// LOOK that flips, so the right loudness is an unmissable warning, not a
/// crash. The ubiquitous quiet case stays quiet by construction: ordinary
/// geometry closing every grouping window empties `plate_stack`, so no
/// enclosing OPEN plate exists and this never runs — that is draw-order
/// design, not a flip.
///
/// Returns the dynamic rule to report, or `None` when the fallback is not the
/// loud case. Pure so the classification is unit-testable; `later_plates` are
/// the open plates emitted after the enclosing host.
#[cfg(debug_assertions)]
fn near_roll_fallback_reason(
    carve: &crate::scene::layout::Rect,
    depth: f32,
    host: &crate::scene::layout::Rect,
    roll: f32,
    later_plates: &[crate::scene::layout::Rect],
    budget_full: bool,
) -> Option<&'static str> {
    // The carve's shaded region — the overlay path's cover-quad inflation.
    let infl = depth * 0.5 + 2.0;
    let (sx0, sy0) = (carve.x - infl, carve.y - infl);
    let (sx1, sy1) = (carve.x + carve.width + infl, carve.y + carve.height + infl);
    // "Near the roll" = the shaded region leaves the host rect deflated by the
    // host's own roll width on any side.
    let near = sx0 < host.x + roll
        || sy0 < host.y + roll
        || sx1 > host.x + host.width - roll
        || sy1 > host.y + host.height - roll;
    if !near {
        return None;
    }
    // The dynamic rules, in the order the grouping guard tests them.
    if budget_full {
        return Some("the feature budget is full");
    }
    if later_plates
        .iter()
        .any(|o| sx0 < o.x + o.width && sx1 > o.x && sy0 < o.y + o.height && sy1 > o.y)
    {
        return Some("a later plate overlaps the carve's shaded region");
    }
    Some("the host's feature run is closed (another plate appended features since)")
}

/// Print a near-roll fallback warning once per distinct message — a carve in a
/// steady layout would otherwise repeat it every frame.
#[cfg(debug_assertions)]
fn plate_carve_warn_once(msg: String) {
    use std::sync::{Mutex, OnceLock};
    static SEEN: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
    if seen.lock().unwrap().insert(msg.clone()) {
        eprintln!("{msg}");
    }
}

pub fn tessellate_display_list(
    dl: &crate::scene::paint::DisplayList,
    sw: f32,
    sh: f32,
    scale: f32,
) -> (Vec<Vertex>, Vec<DlBatch>, Vec<DlImage>, Vec<[f32; 12]>) {
    use crate::scene::material::PlateRole;
    use crate::scene::paint::{Cap, Prim};
    let mut verts: Vec<Vertex> = Vec::new();
    let mut batches: Vec<DlBatch> = Vec::new();
    let mut images: Vec<DlImage> = Vec::new();
    // Carves CSG'd into plates (see Frame2D::plate_features), plus the plate
    // they group into: the most recent Plate/Bevel batch, provided only Text
    // and Image prims (which draw through separate paths anyway) intervene.
    let mut features: Vec<[f32; 12]> = Vec::new();
    // Open carve-host plates, in emission order (innermost candidates last).
    // A STACK, not a single slot: a sibling plate emitted between a root plate
    // and its later carves (a hovered button's opaque fill among transparent
    // ones) must not sever those carves from the root plate they are carved
    // into — that severing rendered every button after the hovered one
    // through the visually-different overlay fallback. Ordinary geometry
    // still closes every open plate (the draw-order rule below).
    let mut plate_stack: Vec<(usize, crate::scene::layout::Rect)> = Vec::new();
    // Which plate last appended a carve feature: a plate's features are
    // addressed as one contiguous [offset, count] run (PlatePush::host), so a
    // plate may only receive MORE features while no other plate has appended
    // any since.
    let mut last_feature_plate: Option<usize> = None;
    // `CCE_PLATE_DEBUG` bookkeeping — see `plate_debug`.
    let dbg_plates = plate_debug();
    let mut dbg_grouped = 0usize;
    let mut dbg_fell_back: Vec<String> = Vec::new();
    let mut dbg_opened = 0usize;
    // Which prim kind closed a still-open grouping window, and how many plates
    // it closed — the answer to "why was there no enclosing plate?".
    let mut dbg_closed_by: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();

    // SDF-lit plate path (shader2d's plate branch) vs the legacy banded vertex
    // shading, plus the frame-constant lighting inputs it pushes per plate.
    let shader_plates = crate::layout::bevel_shader();
    // Light and material come from `scene::relief_shade`, which is also what
    // cce-relief predicts pixels with — one definition, so the editor cannot
    // draw a different material than the renderer applies.
    let plate_light = crate::scene::relief_shade::light_vector();
    // [shading strength (1.0 at the default bevel_depth), specular strength,
    // shininess, curvature/AO strength] — the DE's finish, for the CARVES,
    // which shade whatever is beneath them and so take the host's. A prim
    // that carries a Material (Plate, Bevel, Sphere, Droplet) pushes its own
    // `material.finish` instead. Curvature is kept near the raised path's
    // crest amplitude: the recess shoulder's brightening lands on the same
    // pixels as its specular line, and the two stack — at 0.5 the step read
    // several times hotter than a plate roll.
    let plate_mat = crate::scene::material::Finish::from_style().to_array();

    for item in &dl.items {
        let mut start = verts.len() as u32;
        let mut plate: Option<crate::draw::PlatePush> = None;
        // A frosted flat fill promoted to a zero-depth plate batch (below):
        // it carries a recipe like any plate, but it is ordinary geometry to
        // the carve grouping — it opens no host and closes the open ones.
        let mut promoted = false;
        let mut made_plate: Option<crate::scene::layout::Rect> = None;
        // Blur-behind marker: a prim whose FILL alpha is negative asks the
        // renderer to snapshot the frame-so-far before it draws. Every
        // fill-bearing prim counts — the shader's a<0 branch runs for all of
        // them, and a variant missing here still frosts, but against the
        // stale scene backdrop instead of the frame: a flat tint with no
        // content and no blur, which is how the Dropdown popover (Border)
        // and the menubar panels (Quad) shipped visibly unfrosted while the
        // context menu (Plate) worked.
        let mut blur_behind = matches!(
            &item.prim,
            crate::scene::paint::Prim::Quad { color, .. }
            | crate::scene::paint::Prim::RoundedRect { color, .. } if color[3] < 0.0
        ) || matches!(
            &item.prim,
            crate::scene::paint::Prim::Bevel { material, .. }
            | crate::scene::paint::Prim::Frame { material, .. }
            | crate::scene::paint::Prim::Plate { material, .. }
            | crate::scene::paint::Prim::Droplet { material, .. }
                if material.fill(PlateRole::Nested)[3] < 0.0
        ) || matches!(
            &item.prim,
            crate::scene::paint::Prim::Border { fill, .. } if fill[3] < 0.0
        ) || matches!(
            &item.prim,
            crate::scene::paint::Prim::Fill { material, .. } if material.fill(PlateRole::Nested)[3] < 0.0
        );
        // Logical [cx, cy, r] → the physical-pixel triple the vertex attribute carries.
        let no = item
            .clip_circle
            .map(|c| [c[0] * scale, c[1] * scale, c[2] * scale])
            .unwrap_or([0.0f32, 0.0, 0.0]);
        // Fixed 16-segment fans read as polygons once a circle/arc is pane-sized; scale
        // the fan with the PHYSICAL radius (capped — beyond 128 the chord error is
        // subpixel even on HiDPI).
        let segs = |radius: f32| -> usize { ((radius * scale) as usize).clamp(16, 128) };
        match &item.prim {
            Prim::Text { .. } => continue, // text goes through the glyph/text-span path
            Prim::Image { image, rect, alpha } => {
                images.push(DlImage {
                    image: *image,
                    rect: *rect,
                    alpha: *alpha,
                    at: verts.len() as u32,
                    clip: item.clip,
                });
                continue;
            }
            // A frosted FLAT fill — a `Flat` control face, a menu panel, a
            // popover, an inset plate's face — is a zero-depth plate batch
            // (RFC material § 6.2): the same shader path as every plate, so
            // it carries its own frost recipe instead of a window-wide one,
            // with the configured `corner_shape` and no roll, which is what the
            // tessellated fill drew. The display list is untouched, so the
            // legacy bridges that extract RoundedRects still see one.
            Prim::Quad { rect, color } if shader_plates && color[3] < 0.0 => {
                verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, *color));
                plate = Some(flat_frost_push(rect, (0.0, 0.0, 0.0, 0.0), *color, scale, plate_light, plate_mat));
                promoted = true;
            }
            Prim::RoundedRect { rect, radius, corners, color } if shader_plates && color[3] < 0.0 => {
                let radii = (
                    if corners.0 { *radius } else { 0.0 },
                    if corners.1 { *radius } else { 0.0 },
                    if corners.2 { *radius } else { 0.0 },
                    if corners.3 { *radius } else { 0.0 },
                );
                verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, *color));
                plate = Some(flat_frost_push(rect, radii, *color, scale, plate_light, plate_mat));
                promoted = true;
            }
            Prim::Fill { rect, radii, material } if shader_plates && material.frost.is_frosted() => {
                // A material's flat fill: the frosted promotion above with
                // the MATERIAL's recipe (compression, refraction, radius)
                // instead of the DE default's. Zero depth, the configured
                // corner shape, no host — exactly a promoted RoundedRect.
                let color = material.fill(PlateRole::Nested);
                verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, color));
                let mut p = plate_push_raised(rect, *radii, 0.0, scale, plate_light, plate_mat, false, None);
                let [fz, fw] = material.frost.pack(scale);
                p.host[2] = fz;
                p.host[3] = fw;
                plate = Some(p);
                promoted = true;
            }
            Prim::Fill { rect, radii, material } => {
                // Opaque (or the legacy path): a plain rounded fill.
                let cr = crate::widget::CornerRadii::new(radii.0, radii.1, radii.2, radii.3);
                push_rounded_rect_vertices_corners(rect.x, rect.y, rect.width, rect.height, cr, sw, sh, material.fill(PlateRole::Nested), no, None, &mut verts);
            }
            Prim::Border { rect, radii, fill, border, thickness } if shader_plates && fill[3] < 0.0 => {
                // The fill as its own plate batch, closed here; the stroke
                // follows as ordinary geometry in the batch the tail makes.
                verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, *fill));
                let p = flat_frost_push(rect, *radii, *fill, scale, plate_light, plate_mat);
                let end = verts.len() as u32;
                plate_stack.clear();
                batches.push(DlBatch { scissor: item.clip, clip_rrect: item.clip_rrect, start, end, plate: Some(p), blur_behind: true });
                start = end;
                blur_behind = false;
                let cr = crate::widget::CornerRadii::new(radii.0, radii.1, radii.2, radii.3);
                push_plate_solid_border_vertices(rect.x, rect.y, rect.width, rect.height, cr, *thickness, sw, sh, *border, no, &mut verts);
            }
            Prim::Quad { rect, color } => {
                // Quads honor an active circle clip like circles/arcs do (the
                // Ramp's foam-cell fills draw as clipped strips).
                verts.extend(quad_vertices_with_clip(rect.x, rect.y, rect.width, rect.height, sw, sh, *color, no));
            }
            Prim::RoundedRect { rect, radius, corners, color } => {
                let radii = crate::widget::CornerRadii::new(
                    if corners.0 { *radius } else { 0.0 },
                    if corners.1 { *radius } else { 0.0 },
                    if corners.2 { *radius } else { 0.0 },
                    if corners.3 { *radius } else { 0.0 },
                );
                push_rounded_rect_vertices_corners(rect.x, rect.y, rect.width, rect.height, radii, sw, sh, *color, no, None, &mut verts);
            }
            Prim::Border { rect, radii, fill, border, thickness } => {
                let cr = crate::widget::CornerRadii::new(radii.0, radii.1, radii.2, radii.3);
                push_rounded_rect_vertices_corners(rect.x, rect.y, rect.width, rect.height, cr, sw, sh, *fill, no, None, &mut verts);
                push_plate_solid_border_vertices(rect.x, rect.y, rect.width, rect.height, cr, *thickness, sw, sh, *border, no, &mut verts);
            }
            Prim::Glow { rect, radius, reach, color } => {
                push_glow_vertices(rect.x, rect.y, rect.width, rect.height, *radius, *reach, sw, sh, *color, no, &mut verts);
            }
            Prim::Bevel { rect, radii, material, depth, tint } if shader_plates => {
                let color = material.fill(PlateRole::Nested);
                let mat = material.finish.to_array();
                // SDF-lit raised plate: one cover quad; the shader owns fill,
                // roll shading, corners, and silhouette AA. Nominal corner
                // radii (scale_corners false): a Bevel is a WIDGET-scale plate
                // whose silhouette must match the nominal-radius squircles of
                // the controls around it — only window-scale `Plate`s get the
                // curvature-matched span.
                verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, color));
                let mut p = plate_push_raised(rect, *radii, *depth, scale, plate_light, mat, false, None);
                // The plate's own frost recipe rides host.zw (see PlatePush).
                let [fz, fw] = material.frost.pack(scale);
                p.host[2] = fz;
                p.host[3] = fw;
                // w = 1 marks an accent-tinted plate (the focused-pane
                // treatment): the shader keeps the roll's light and shadow
                // and recolours them — light toward the tint, shadow toward
                // a dark tint — matching the free-carve path's tinted-well
                // convention. Neutral white keeps w = 0 (a no-op multiply).
                let full = if *tint == [1.0, 1.0, 1.0] { 0.0 } else { 1.0 };
                p.specular_tint = [tint[0], tint[1], tint[2], full];
                plate = Some(p);
                made_plate = Some(*rect);
            }
            Prim::Frame { rect, hole, hole_radii, material, depth } if shader_plates => {
                // The Bevel branch turned inside out (shader MODE_FRAME): the
                // cover quad is the face's bound, the SDF box is the HOLE,
                // and the shader reads the distance outside it as the
                // plate's depth. Nominal corner radii, as a Bevel's. A carve
                // host over `rect`, like any filled plate.
                let color = material.fill(PlateRole::Nested);
                let mat = material.finish.to_array();
                verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, color));
                let mut p = plate_push_raised(hole, *hole_radii, *depth, scale, plate_light, mat, false, None);
                let [fz, fw] = material.frost.pack(scale);
                p.host[2] = fz;
                p.host[3] = fw;
                p.mode = 17.0; // MODE_FRAME
                plate = Some(p);
                made_plate = Some(*rect);
            }
            Prim::Plate { rect, radii, material, depth, shape } if shader_plates => {
                let color = material.fill(PlateRole::Nested);
                let mat = material.finish.to_array();
                if *depth < 0.0 {
                    // Negative depth = fill-less roll overlay (MODE_ROLL): the
                    // window-edge roll shading alone, screened over whatever is
                    // beneath — for a root plate whose face is not a fill (the
                    // designer's 3D canvas). The cover quad carries no color,
                    // and the batch is NOT opened as a carve host: an overlay
                    // owns no surface for a CSG feature to cut into.
                    verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, [0.0; 4]));
                    let mut p = plate_push_raised(rect, *radii, -*depth, scale, plate_light, mat, true, *shape);
                    p.mode = 11.0; // MODE_ROLL
                    plate = Some(p);
                } else {
                    // Same lit-plate branch; the cover quad is the exact rect so the
                    // silhouette and the compositor's rounded window corners agree.
                    verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, color));
                    let mut p = plate_push_raised(rect, *radii, *depth, scale, plate_light, mat, true, *shape);
                    let [fz, fw] = material.frost.pack(scale);
                    p.host[2] = fz;
                    p.host[3] = fw;
                    plate = Some(p);
                    made_plate = Some(*rect);
                }
            }
            // A sunken well ending in a flush run (see `Prim::Field`): one
            // outline, one overlay — never grouped, its profile is not a
            // monotonic step. The cover quad inflates by half the wall, as a
            // free carve's does; the host-box slot carries the run's two
            // ends (physical px), since nothing fades against a host here.
            Prim::Field { rect, radii, depth, split, end, tint } if shader_plates => {
                let infl = *depth * 0.5 + 2.0;
                verts.extend(quad_vertices(
                    rect.x - infl, rect.y - infl,
                    rect.width + 2.0 * infl, rect.height + 2.0 * infl,
                    sw, sh, [0.0; 4],
                ));
                let mut p = plate_push_raised(rect, *radii, *depth, scale, plate_light, plate_mat, false, None);
                p.mode = 16.0; // MODE_FIELD
                if let Some(t) = tint {
                    p.specular_tint = [t[0], t[1], t[2], 1.0];
                }
                p.host = [*split * scale, *end * scale, 0.0, 0.0];
                plate = Some(p);
            }
            Prim::Recess { rect, radii, depth, edges, .. }
            | Prim::Boss { rect, radii, depth, edges, .. }
            | Prim::Ridge { rect, radii, depth, edges }
            | Prim::Trough { rect, radii, depth, edges, .. }
                if shader_plates =>
            {
                let tint = match &item.prim {
                    Prim::Recess { tint, .. } => *tint,
                    Prim::Boss { tint, .. } => *tint,
                    Prim::Trough { tint, .. } => *tint,
                    _ => None,
                };
                // Recess carves down into the surface; Boss raises a plateau out
                // of it (same machinery, depth sign flipped); Ridge is a raised
                // rim straddling the boundary and Trough the sunken valley twin
                // (their own overlay profiles — never grouped, the CSG features
                // only model monotonic steps).
                let mode = match &item.prim {
                    Prim::Boss { .. } => 3.0f32,
                    Prim::Ridge { .. } => 4.0,
                    Prim::Trough { .. } => 9.0,
                    _ => 2.0,
                };
                let raised = mode > 2.5;
                // Grouped into the enclosing plate whenever one is live: the
                // carve becomes a CSG feature of that plate's single draw —
                // exact composite shading, real junctions at the plate's rolled
                // perimeter — instead of a shading overlay (the fallback below).
                //
                // Edge-suppressed carves NEVER group: a suppressed wall's rect
                // extends past the carve (below), relying on the overlay cover
                // quad to keep that shading out of the drawn pixels — a clip
                // the plate's whole-surface draw does not have, so grouped it
                // smears the extended walls across the plate. Union pieces
                // (section wells, a spinbox's field and button run) are
                // exactly these.
                // A tinted carve also never groups: a CSG feature is geometry only,
                // so the tint could only land on the whole plate's specular.
                let full_ring = *edges == (true, true, true, true);
                let host_plate = if mode < 3.5 && full_ring && tint.is_none() && features.len() < crate::draw::MAX_PLATE_FEATURES {
                    // The carve's shaded region, for the occlusion test below
                    // (the overlay path's cover-quad inflation).
                    let infl = *depth * 0.5 + 2.0;
                    let (sx0, sy0) = (rect.x - infl, rect.y - infl);
                    let (sx1, sy1) = (rect.x + rect.width + infl, rect.y + rect.height + infl);
                    plate_stack
                        .iter()
                        .enumerate()
                        .rev()
                        .find(|(si, (bi, prect))| {
                            let inside = rect.x >= prect.x - 0.5
                                && rect.y >= prect.y - 0.5
                                && rect.x + rect.width <= prect.x + prect.width + 0.5
                                && rect.y + rect.height <= prect.y + prect.height + 0.5;
                            if !inside {
                                return false;
                            }
                            // Pixels drawn since this plate (a LATER plate in the
                            // stack) must not overlap the carve — its shading would
                            // land beneath them in this plate's earlier draw.
                            if plate_stack[si + 1..].iter().any(|(_, orect)| {
                                sx0 < orect.x + orect.width
                                    && sx1 > orect.x
                                    && sy0 < orect.y + orect.height
                                    && sy1 > orect.y
                            }) {
                                return false;
                            }
                            // Contiguity: only the last feature-receiving plate (or
                            // one with no features yet) may take another.
                            batches[*bi].plate.as_ref().is_some_and(|p| p.host[1] == 0.0)
                                || last_feature_plate == Some(*bi)
                        })
                        .map(|(_, &(bi, _))| bi)
                } else {
                    None
                };
                // Debug-build loudness for the silent grouped→overlay flip —
                // see `near_roll_fallback_reason` on what qualifies and why
                // this warns instead of panicking.
                #[cfg(debug_assertions)]
                if host_plate.is_none() && mode < 3.5 && full_ring && tint.is_none() {
                    let enclosing = plate_stack.iter().enumerate().rev().find(|(_, (_, p))| {
                        rect.x >= p.x - 0.5
                            && rect.y >= p.y - 0.5
                            && rect.x + rect.width <= p.x + p.width + 0.5
                            && rect.y + rect.height <= p.y + p.height + 0.5
                    });
                    if let Some((si, &(bi, prect))) = enclosing {
                        // Host roll width rides the push's light.w (physical px).
                        let roll = batches[bi].plate.as_ref().map_or(0.0, |p| p.light[3]) / scale;
                        let later: Vec<crate::scene::layout::Rect> =
                            plate_stack[si + 1..].iter().map(|&(_, r)| r).collect();
                        let budget_full = features.len() >= crate::draw::MAX_PLATE_FEATURES;
                        if let Some(why) =
                            near_roll_fallback_reason(rect, *depth, &prect, roll, &later, budget_full)
                        {
                            let kind = if mode > 2.5 { "boss" } else { "recess" };
                            plate_carve_warn_once(format!(
                                "plate-carve: near-roll {kind} ({:.0},{:.0} {:.0}x{:.0}) lost grouping — {why}; \
                                 its junction with the host plate's roll shades through the overlay fallback, \
                                 visually different from grouped frames (CCE_PLATE_DEBUG=1 traces verdicts) \
                                 [debug-build warning, printed once]",
                                rect.x, rect.y, rect.width, rect.height
                            ));
                        }
                    }
                }
                if dbg_plates {
                    match host_plate {
                        Some(_) => dbg_grouped += 1,
                        None => {
                            // Re-derive WHY, in the same order the guard tests
                            // them. Debug-only: the hot path above is untouched.
                            let kind = match &item.prim {
                                Prim::Boss { .. } => "boss",
                                Prim::Ridge { .. } => "ridge",
                                Prim::Trough { .. } => "trough",
                                _ => "recess",
                            };
                            let infl = *depth * 0.5 + 2.0;
                            let (sx0, sy0) = (rect.x - infl, rect.y - infl);
                            let (sx1, sy1) = (rect.x + rect.width + infl, rect.y + rect.height + infl);
                            let enclosing: Vec<usize> = plate_stack
                                .iter()
                                .enumerate()
                                .filter(|(_, (_, p))| {
                                    rect.x >= p.x - 0.5
                                        && rect.y >= p.y - 0.5
                                        && rect.x + rect.width <= p.x + p.width + 0.5
                                        && rect.y + rect.height <= p.y + p.height + 0.5
                                })
                                .map(|(si, _)| si)
                                .collect();
                            let occluded = |si: usize| {
                                plate_stack[si + 1..].iter().any(|(_, o)| {
                                    sx0 < o.x + o.width && sx1 > o.x && sy0 < o.y + o.height && sy1 > o.y
                                })
                            };
                            let why = if mode >= 3.5 {
                                "ridge — never groups (its bump profile is not a monotonic step)".into()
                            } else if !full_ring {
                                format!("edge-suppressed {edges:?} — the extended wall would smear across the host")
                            } else if tint.is_some() {
                                "tinted — a CSG feature is geometry only, it carries no color".into()
                            } else if features.len() >= crate::draw::MAX_PLATE_FEATURES {
                                format!("feature budget full ({} used)", features.len())
                            } else if enclosing.is_empty() {
                                format!("no enclosing plate ({} open)", plate_stack.len())
                            } else if enclosing.iter().all(|&si| occluded(si)) {
                                "a later plate overlaps this carve's shaded region".into()
                            } else {
                                "host plate's feature run is closed (another carve appended since)".into()
                            };
                            dbg_fell_back.push(format!(
                                "  overlay: {kind} ({:.0},{:.0} {:.0}x{:.0}) — {why}",
                                rect.x, rect.y, rect.width, rect.height
                            ));
                        }
                    }
                }
                if let Some(bi) = host_plate {
                    {
                        // A wall the carve shares with the plate's edge extends
                        // past the plate, so the carve has no wall there.
                        let ext = *depth + 4.0;
                        let (mut x0, mut y0) = (rect.x, rect.y);
                        let (mut x1, mut y1) = (rect.x + rect.width, rect.y + rect.height);
                        if !edges.0 { y0 -= ext; }
                        if !edges.1 { x1 += ext; }
                        if !edges.2 { y1 += ext; }
                        if !edges.3 { x0 -= ext; }
                        let t_px = *depth * scale;
                        // The carve's drop: the material's pinned height, else
                        // the analytic ratio of the wall saturating at the DE's
                        // roll width (`layout::carve_depth_px` states the rule
                        // once for this path and the shader's free carves).
                        let k_mag = crate::layout::carve_depth_px(*depth) * scale;
                        // Negative depth = raised (Boss); the shader's summed
                        // slope vectors and curvature sign follow it.
                        let k_px = if raised { -k_mag } else { k_mag };
                        if let Some(p) = batches[bi].plate.as_mut() {
                            if p.host[1] == 0.0 {
                                p.host[0] = features.len() as f32;
                            }
                            p.host[1] += 1.0;
                        }
                        last_feature_plate = Some(bi);
                        features.push([
                            (x0 + x1) * 0.5 * scale,
                            (y0 + y1) * 0.5 * scale,
                            (x1 - x0) * 0.5 * scale,
                            (y1 - y0) * 0.5 * scale,
                            radii.0 * scale,
                            radii.1 * scale,
                            radii.2 * scale,
                            radii.3 * scale,
                            t_px,
                            k_px,
                            0.0,
                            0.0,
                        ]);
                        continue;
                    }
                }
                // Overlay-only carve: the cover quad inflates by half the roll
                // width (the step straddles the boundary) and carries no color —
                // the shader emits translucent white/black over what's beneath.
                let infl = *depth * 0.5 + 2.0;
                verts.extend(quad_vertices(
                    rect.x - infl, rect.y - infl,
                    rect.width + 2.0 * infl, rect.height + 2.0 * infl,
                    sw, sh, [0.0; 4],
                ));
                // A suppressed wall is pushed past the cover quad, so its
                // shading falls outside the drawn pixels (see Prim::Recess on
                // why a flush region is a step, not a trough).
                let ext = *depth + 4.0;
                let (mut x0, mut y0) = (rect.x, rect.y);
                let (mut x1, mut y1) = (rect.x + rect.width, rect.y + rect.height);
                if !edges.0 { y0 -= ext; }
                if !edges.1 { x1 += ext; }
                if !edges.2 { y1 += ext; }
                if !edges.3 { x0 -= ext; }
                let sdf_rect = crate::scene::layout::Rect { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
                let mut p = plate_push_raised(&sdf_rect, *radii, *depth, scale, plate_light, plate_mat, false, None);
                p.mode = mode;
                // w = 1.0 flags the free-carve shader path to composite its
                // light in the tint and its shadow in a dark tint instead of
                // white and black (plates leave w at 0.0).
                if let Some(t) = tint {
                    p.specular_tint = [t[0], t[1], t[2], 1.0];
                }
                // Host-plate box for the roll fade: a suppressed wall means the
                // recess runs flush to the host's edge there, so that side of
                // the box sits at the original rect edge; enabled walls face
                // host interior, pushed to ±1e5 so no fade applies.
                const FAR: f32 = 1e5;
                let (hx0, hy0) = (
                    if edges.3 { rect.x - FAR } else { rect.x },
                    if edges.0 { rect.y - FAR } else { rect.y },
                );
                let (hx1, hy1) = (
                    if edges.1 { rect.x + rect.width + FAR } else { rect.x + rect.width },
                    if edges.2 { rect.y + rect.height + FAR } else { rect.y + rect.height },
                );
                p.host = [
                    (hx0 + hx1) * 0.5 * scale,
                    (hy0 + hy1) * 0.5 * scale,
                    (hx1 - hx0) * 0.5 * scale,
                    (hy1 - hy0) * 0.5 * scale,
                ];
                plate = Some(p);
            }
            Prim::Bevel { rect, radii, material, depth, tint: _ } => {
                let color = material.fill(PlateRole::Nested);
                // Full-size fill: the lip is now a shading overlay, not a paint of the
                // outer ring, so the fill must cover the whole rect (the old inset fill
                // would leave the ring showing whatever lay beneath).
                let corners = crate::widget::CornerRadii {
                    top_left: radii.0, top_right: radii.1,
                    bottom_right: radii.2, bottom_left: radii.3,
                };
                push_rounded_rect_vertices_corners(rect.x, rect.y, rect.width, rect.height, corners, sw, sh, color, no, None, &mut verts);
                push_plate_bevel_vertices(rect.x, rect.y, rect.width, rect.height, radii.0, *depth, sw, sh, color, no, &mut verts);
            }
            // The legacy banded path has no inside-out SDF: the face square,
            // and only below the hole, so the coves' rows are left to what
            // is beneath (A/B comparison path only).
            Prim::Frame { rect, hole, material, .. } => {
                let color = material.fill(PlateRole::Nested);
                let y0 = rect.y.max(hole.y + hole.height);
                let y1 = rect.y + rect.height;
                if y1 > y0 {
                    verts.extend(quad_vertices(rect.x, y0, rect.width, y1 - y0, sw, sh, color));
                }
            }
            Prim::Plate { rect, radii, material, depth, .. } => {
                let color = material.fill(PlateRole::Nested);
                if *depth < 0.0 {
                    // Fill-less roll overlay (negative-depth sentinel): the banded
                    // legacy tessellation has no overlay compositing, so the roll
                    // is simply absent here — the A/B path draws nothing rather
                    // than a wrong fill.
                    continue;
                }
                // Fill at full size (no inset — see Prim::Plate), then light the face,
                // then roll the perimeter. The lip rides on top of the fill's outer band
                // rather than replacing it, so the plate's silhouette and the
                // compositor's rounded window corners still agree exactly.
                let corners = crate::widget::CornerRadii {
                    top_left: radii.0, top_right: radii.1,
                    bottom_right: radii.2, bottom_left: radii.3,
                };
                push_rounded_rect_vertices_corners(
                    rect.x, rect.y, rect.width, rect.height, corners, sw, sh, color, no, None, &mut verts,
                );
                push_plate_face_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, no, &mut verts);
                push_bevel_edge_vertices_radii(
                    rect.x, rect.y, rect.width, rect.height, *radii, *depth,
                    sw, sh, color, no, 1.0, &mut verts,
                );
            }
            Prim::Recess { rect, radii, depth, edges, .. } => {
                // Edges only — no fill: the shading is an overlay, so whatever is painted
                // below (fill, rim gradient, blur) shows through the carve modulated
                // rather than repainted. `light_sign = -1.0` shadows the lit-facing edges,
                // which is the raised->recessed inversion.
                push_bevel_edge_vertices_banded(
                    rect.x, rect.y, rect.width, rect.height, *radii, *depth,
                    sw, sh, [0.0; 4], no, -1.0, default_bevel_bands(*depth), *edges,
                    EdgeKind::Step, &mut verts,
                );
            }
            Prim::Boss { rect, radii, depth, edges, .. } => {
                // Legacy raised step: the recess overlay with the light sign upright.
                push_bevel_edge_vertices_banded(
                    rect.x, rect.y, rect.width, rect.height, *radii, *depth,
                    sw, sh, [0.0; 4], no, 1.0, default_bevel_bands(*depth), *edges,
                    EdgeKind::Step, &mut verts,
                );
            }
            Prim::Ridge { rect, radii, depth, edges } => {
                // Legacy approximation: a raised step up at the boundary plus a
                // recessed step down half a width in (the banded machinery has no
                // bump profile; the double-pass hot crest is accepted here — the
                // legacy path exists only for A/B comparison).
                let half = *depth * 0.5;
                push_bevel_edge_vertices_banded(
                    rect.x, rect.y, rect.width, rect.height, *radii, half,
                    sw, sh, [0.0; 4], no, 1.0, default_bevel_bands(half), *edges,
                    EdgeKind::Step, &mut verts,
                );
                let ir = (radii.0 - half).max(0.0);
                push_bevel_edge_vertices_banded(
                    rect.x + half, rect.y + half,
                    rect.width - *depth, rect.height - *depth,
                    (ir, ir, ir, ir), half,
                    sw, sh, [0.0; 4], no, -1.0, default_bevel_bands(half), *edges,
                    EdgeKind::Step, &mut verts,
                );
            }
            Prim::Trough { rect, radii, depth, edges, .. } => {
                // Legacy approximation, the Ridge arm's two steps with the light
                // signs swapped: down at the boundary, back up half a width in.
                // The banded machinery has no valley profile, so this is the old
                // stacked look — accepted here, as the legacy path exists only
                // for A/B comparison against the SDF one.
                let half = *depth * 0.5;
                push_bevel_edge_vertices_banded(
                    rect.x, rect.y, rect.width, rect.height, *radii, half,
                    sw, sh, [0.0; 4], no, -1.0, default_bevel_bands(half), *edges,
                    EdgeKind::Step, &mut verts,
                );
                let ir = (radii.0 - half).max(0.0);
                push_bevel_edge_vertices_banded(
                    rect.x + half, rect.y + half,
                    rect.width - *depth, rect.height - *depth,
                    (ir, ir, ir, ir), half,
                    sw, sh, [0.0; 4], no, 1.0, default_bevel_bands(half), *edges,
                    EdgeKind::Step, &mut verts,
                );
            }
            Prim::Field { rect, radii, depth, split, end, .. } => {
                // Legacy approximation: the two-box form `Prim::Field`
                // replaced — a well either side of the run a step down, the
                // run the Trough arm's down-then-up stack. The banded
                // machinery has no blended outline, and the legacy path
                // exists only for A/B comparison.
                let all = (true, true, true, true);
                let (fl, fr) = (rect.x, rect.x + rect.width);
                let (well_l, well_r) = (*split > fl, *end < fr);
                let rx = split.max(fl);
                let rw = (end.min(fr) - rx).max(0.0);
                if well_l {
                    push_bevel_edge_vertices_banded(
                        fl, rect.y, rx - fl, rect.height, (radii.0, 0.0, 0.0, radii.3), *depth,
                        sw, sh, [0.0; 4], no, -1.0, default_bevel_bands(*depth), all,
                        EdgeKind::Step, &mut verts,
                    );
                }
                if well_r {
                    push_bevel_edge_vertices_banded(
                        rx + rw, rect.y, fr - rx - rw, rect.height, (0.0, radii.1, radii.2, 0.0), *depth,
                        sw, sh, [0.0; 4], no, -1.0, default_bevel_bands(*depth), all,
                        EdgeKind::Step, &mut verts,
                    );
                }
                // A run's own corners where it reaches the outline; square at a seam.
                let (l0, l3) = if well_l { (0.0, 0.0) } else { (radii.0, radii.3) };
                let (r1, r2) = if well_r { (0.0, 0.0) } else { (radii.1, radii.2) };
                let half = *depth * 0.5;
                push_bevel_edge_vertices_banded(
                    rx, rect.y, rw, rect.height, (l0, r1, r2, l3), half,
                    sw, sh, [0.0; 4], no, -1.0, default_bevel_bands(half), all,
                    EdgeKind::Step, &mut verts,
                );
                let ir = if well_r { 0.0 } else { (radii.1 - half).max(0.0) };
                let il = if well_l { 0.0 } else { (radii.0 - half).max(0.0) };
                push_bevel_edge_vertices_banded(
                    rx + half, rect.y + half, rw - *depth, rect.height - *depth, (il, ir, ir, il), half,
                    sw, sh, [0.0; 4], no, 1.0, default_bevel_bands(half), all,
                    EdgeKind::Step, &mut verts,
                );
            }
            Prim::Arc { cx, cy, radius, thickness, start: sa, end: ea, color } => {
                push_arc_background_vertices(*cx, *cy, *radius, *thickness, *sa, *ea, sw, sh, *color, segs(*radius), no, &mut verts);
            }
            Prim::ArcShaded { cx, cy, radius, thickness, start: sa, end: ea, inner, crest, outer } => {
                push_arc_shaded_vertices(*cx, *cy, *radius, *thickness, *sa, *ea, sw, sh, *inner, *crest, *outer, segs(*radius), no, &mut verts);
            }
            Prim::Vector { x1, y1, x2, y2, thickness, color, cap } => {
                let lc = match cap {
                    Cap::Flat => LineCap::Flat,
                    Cap::Round => LineCap::Round,
                    Cap::Arrow => LineCap::Arrow,
                };
                verts.extend(vector_vertices(*x1, *y1, *x2, *y2, *thickness, sw, sh, *color, lc));
            }
            Prim::Circle { cx, cy, radius, color } => {
                if item.clip_circle.is_none() && *radius > 1.5 {
                    // Cover quad with the disc itself as the (feathered) circle
                    // clip: a per-pixel smooth silhouette instead of a hard-edged
                    // fan. The quad overhangs by 1px for the feather. Only when
                    // no ancestor clip holds the slot — then it's the fan path.
                    let own = [cx * scale, cy * scale, radius * scale];
                    let d = *radius + 1.0;
                    verts.extend(quad_vertices_with_clip(
                        cx - d, cy - d, 2.0 * d, 2.0 * d, sw, sh, *color, own,
                    ));
                } else {
                    verts.extend(circle_vertices(*cx, *cy, *radius, sw, sh, *color, segs(*radius), no));
                }
            }
            Prim::Sphere { cx, cy, radius, material } if shader_plates => {
                let color = material.fill(PlateRole::Nested);
                let mat = material.finish.to_array();
                // A hemisphere lit per pixel by the plate branch (mode 5): one
                // cover quad, its own never-merged batch. The quad overhangs
                // the disc by 1px for the shader's silhouette anti-aliasing.
                let d = *radius + 1.0;
                verts.extend(quad_vertices(cx - d, cy - d, 2.0 * d, 2.0 * d, sw, sh, color));
                plate = Some(crate::draw::PlatePush {
                    // Center + radius in physical px; the SDF box machinery is
                    // unused in this mode, so .w is free.
                    rect: [cx * scale, cy * scale, radius * scale, 0.0],
                    radii: [0.0; 4],
                    light: [plate_light[0], plate_light[1], plate_light[2], 0.0],
                    material: mat,
                    host: [0.0; 4],
                    specular_tint: [1.0, 1.0, 1.0, 0.0],
                    mode: 5.0,
                    shape: 2.0,
                });
            }
            Prim::Sphere { cx, cy, radius, material } => {
                let color = material.fill(PlateRole::Nested);
                // Legacy path: the flat disc, exactly a Circle.
                verts.extend(circle_vertices(*cx, *cy, *radius, sw, sh, color, segs(*radius), no));
            }
            Prim::DropletScrim { rect, material, spec, feather } if shader_plates => {
                let color = material.fill(PlateRole::Nested);
                let mat = material.finish.to_array();
                // Shader mode 12: the droplet's own SDF, filled flat and
                // feathered inward. No contact shadow, so unlike the lit drop
                // the cover quad is exactly the box — a scrim never draws
                // outside the silhouette.
                let g = droplet_geom(rect, spec);
                verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, color));
                plate = Some(crate::draw::PlatePush {
                    rect: [
                        (rect.x + rect.width * 0.5) * scale,
                        (rect.y + rect.height * 0.5) * scale,
                        g.hx * scale,
                        g.hy * scale,
                    ],
                    radii: [g.sag * scale, g.br * scale, g.bw * scale, g.k * scale],
                    // p_light.w carries the FEATHER here; mode 12 returns
                    // before the shading band it otherwise holds is read.
                    light: [plate_light[0], plate_light[1], plate_light[2], feather.max(0.001) * scale],
                    material: [mat[0], 0.0, 0.0, 0.0],
                    host: [g.sr * scale, 0.0, 0.0, g.ar * scale],
                    specular_tint: [0.0, 0.0, 0.0, g.bow * scale],
                    mode: 12.0,
                    shape: spec.curve.clamp(2.0, 6.0),
                });
            }
            Prim::Droplet { rect, material, spec } if shader_plates => {
                let color = material.fill(PlateRole::Nested);
                let mat = material.finish.to_array();
                // A water droplet lit by shader mode 10: one cover quad; the
                // shader owns silhouette (sheet ∪smin belly), dome shading,
                // fresnel rim and thin-edge clarity. The spec's height
                // fractions resolve against the concrete rect here, clamped so
                // small or narrow boxes stay well-formed (a belly wider than
                // the box would turn the SDF interior inside out).
                // The cover quad grows sideways and BELOW the box by the
                // contact shadow's reach — shadow fragments live outside the
                // silhouette, so they need covered pixels to shade.
                let g = droplet_geom(rect, spec);
                let (hx, hy, sag, br, bw, k, sr, ar, band, bow, sh_reach) =
                    (g.hx, g.hy, g.sag, g.br, g.bw, g.k, g.sr, g.ar, g.band, g.bow, g.sh_reach);
                verts.extend(quad_vertices(
                    rect.x - sh_reach,
                    rect.y,
                    rect.width + 2.0 * sh_reach,
                    rect.height + sh_reach,
                    sw, sh, color,
                ));
                plate = Some(crate::draw::PlatePush {
                    rect: [
                        (rect.x + rect.width * 0.5) * scale,
                        (rect.y + rect.height * 0.5) * scale,
                        hx * scale,
                        hy * scale,
                    ],
                    radii: [sag * scale, br * scale, bw * scale, k * scale],
                    light: [plate_light[0], plate_light[1], plate_light[2], band * scale],
                    // Slots y/z/w feed roll_spec and the rim term directly:
                    // a droplet's material carries its own gleam/shine/rim
                    // there (`DropletSpec::finish`; a drop is wetter than the
                    // DE's plates), so this is the material's finish like any
                    // plate's.
                    material: mat,
                    host: [sr * scale, spec.clarity.clamp(0.0, 1.0), spec.dome, ar * scale],
                    // Droplet glints are always white, so the tint RGB slots
                    // carry droplet params instead: x = core density,
                    // y = contact-shadow reach px, z = shadow strength.
                    specular_tint: [
                        spec.core.clamp(0.0, 2.0),
                        sh_reach * scale,
                        spec.shadow.clamp(0.0, 1.0),
                        bow * scale,
                    ],
                    mode: 10.0,
                    shape: spec.curve.clamp(2.0, 6.0),
                });
            }
            Prim::DropletScrim { rect, material, spec, .. } => {
                let color = material.fill(PlateRole::Nested);
                // Legacy banded path: no SDF to feather against, so the scrim
                // degrades to the same flat outline the drop itself does —
                // hard-edged, but present. A prim with no arm here VANISHES.
                let cap = (rect.height * 0.5).min(rect.width * 0.5);
                let sr = (spec.sheet_r.clamp(0.0, 1.0) * rect.height).min(cap);
                let ar = (spec.attach.clamp(0.0, 1.0) * rect.height).min(cap);
                let radii = crate::widget::CornerRadii::new(ar, ar, sr, sr);
                push_rounded_rect_vertices_corners(rect.x, rect.y, rect.width, rect.height, radii, sw, sh, color, no, None, &mut verts);
            }
            Prim::Droplet { rect, material, spec } => {
                let color = material.fill(PlateRole::Nested);
                // Legacy banded path: the flat drop outline — attach-tapered
                // top, round bottom. Degrades the material but keeps the
                // silhouette (a prim with no arm here VANISHES, it doesn't
                // degrade — see Ridge/Groove above).
                let cap = (rect.height * 0.5).min(rect.width * 0.5);
                let sr = (spec.sheet_r.clamp(0.0, 1.0) * rect.height).min(cap);
                let ar = (spec.attach.clamp(0.0, 1.0) * rect.height).min(cap);
                let radii = crate::widget::CornerRadii::new(ar, ar, sr, sr);
                push_rounded_rect_vertices_corners(rect.x, rect.y, rect.width, rect.height, radii, sw, sh, color, no, None, &mut verts);
            }
            Prim::ConcaveFillet { cx, cy, radius, depth, start: a0, raised } if shader_plates => {
                // A quarter-arc carve wall (shader mode 6/7): one cover quad
                // over the wedge's reach; the wall straddles the arc by ±t/2
                // like every carve boundary. p_rect carries centre + radius,
                // p_radii.x the wedge start angle. Host box pushed far out —
                // an inside-corner fillet never fades.
                let m = *depth * 0.5 + 2.0;
                let r = *radius + m;
                verts.extend(quad_vertices(cx - r, cy - r, 2.0 * r, 2.0 * r, sw, sh, [0.0; 4]));
                plate = Some(crate::draw::PlatePush {
                    rect: [cx * scale, cy * scale, *radius * scale, 0.0],
                    radii: [*a0, 0.0, 0.0, 0.0],
                    light: [plate_light[0], plate_light[1], plate_light[2], *depth * scale],
                    material: plate_mat,
                    host: [0.0, 0.0, 1e6, 1e6],
                    specular_tint: [1.0, 1.0, 1.0, 0.0],
                    mode: if *raised { 7.0 } else { 6.0 },
                    shape: crate::layout::corner_shape(),
                });
            }
            // Legacy banded path has no radial wall — the composed corner
            // stays square there (A/B comparison path only).
            Prim::ConcaveFillet { .. } => {}
            Prim::Groove { a, b, width, depth, host, strength } if shader_plates => {
                // A slab carve about the line a–b (shader mode 8): the cover
                // quad is the segment's bounding box grown by the groove's own
                // half-width plus the wall's reach. Off-band corners of that
                // box sit at u = 1 (plateau), so the box overhang shades
                // nothing — the slab is what bounds the mark, not the quad.
                let m = *width * 0.5 + *depth * 0.5 + 2.0;
                let (x0, x1) = (a.0.min(b.0) - m, a.0.max(b.0) + m);
                let (y0, y1) = (a.1.min(b.1) - m, a.1.max(b.1) + m);
                verts.extend(quad_vertices(x0, y0, x1 - x0, y1 - y0, sw, sh, [0.0; 4]));
                // Unit normal of the line — the direction the slab's distance is
                // measured along. A degenerate segment falls back to vertical so
                // a zero-length groove is a no-op wall rather than a NaN.
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let len = (dx * dx + dy * dy).sqrt();
                let n = if len > 1e-4 { (-dy / len, dx / len) } else { (1.0, 0.0) };
                plate = Some(crate::draw::PlatePush {
                    // Centre + slab half-width in physical px; .w unused.
                    rect: [
                        (a.0 + b.0) * 0.5 * scale,
                        (a.1 + b.1) * 0.5 * scale,
                        *width * 0.5 * scale,
                        0.0,
                    ],
                    radii: [n.0, n.1, 0.0, 0.0],
                    light: [plate_light[0], plate_light[1], plate_light[2], *depth * scale],
                    // The finish's shading, specular and AO, at the groove's
                    // strength; shininess is a shape, not an amount.
                    material: {
                        let s = strength.clamp(0.0, 1.0);
                        [plate_mat[0] * s, plate_mat[1] * s, plate_mat[2], plate_mat[3] * s]
                    },
                    host: [
                        (host.x + host.width * 0.5) * scale,
                        (host.y + host.height * 0.5) * scale,
                        host.width * 0.5 * scale,
                        host.height * 0.5 * scale,
                    ],
                    specular_tint: [1.0, 1.0, 1.0, 0.0],
                    mode: 8.0,
                    shape: crate::layout::corner_shape(),
                });
            }
            Prim::Groove { a, b, width, depth, host: _, strength } => {
                // Legacy approximation. The banded tessellators walk BOX edges —
                // exactly the axis-aligned assumption a groove exists to escape —
                // so the walls are drawn directly as two feathered lines meeting
                // at the centerline: the engraved-line fake, one half in shadow
                // and one lit. Coarser than the SDF (no profile curve, no host
                // fade), but this path exists for A/B comparison, and drawing
                // NOTHING would silently delete the mark rather than degrade it
                // — see `Prim::Ridge` above, which accepts a hot crest for the
                // same reason.
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let len = (dx * dx + dy * dy).sqrt();
                if len < 0.001 {
                    continue;
                }
                let n = (-dy / len, dx / len);
                // Same convention as `push_bevel_edge_vertices_banded`: the
                // light folded through `light_sign` (-1.0 — a groove is a
                // carve), dotted with each wall's OUTWARD normal, amplitude on
                // `bevel_depth`. So a groove re-lights with the DE's light
                // instead of hardcoding which side is dark.
                let rad = crate::layout::light_source_position();
                let (lx, ly) = (-rad.cos(), rad.sin());
                let v = crate::layout::bevel_depth() * (n.0 * lx + n.1 * ly);
                // Each wall covers its own half, centreline to outer edge —
                // abutting rather than overlapping. The SDF gets away with
                // walls that overlap across a sub-pixel floor because it is one
                // evaluation of |distance|; two opposite-signed overlays would
                // just blend to mud.
                let half = (*width * 0.5 + *depth * 0.5).max(0.5);
                for side in [1.0f32, -1.0] {
                    let sv = v * side;
                    let mut c = if sv >= 0.0 { overlay_light(sv) } else { overlay_dark(sv) };
                    c[3] *= strength.clamp(0.0, 1.0);
                    if c[3] <= 0.0 {
                        continue;
                    }
                    let off = side * half * 0.5;
                    push_feathered_line_vertices(
                        a.0 + n.0 * off, a.1 + n.1 * off,
                        b.0 + n.0 * off, b.1 + n.1 * off,
                        half, sw, sh, c, &mut verts,
                    );
                }
            }
            Prim::Lattice { rect, period, origin, cell, radius, depth } if shader_plates => {
                // A periodic well field (shader mode 13): one cover quad over
                // `rect`; the shader folds each pixel into the period and
                // measures the nearest cell, so the whole lattice is a single
                // evaluation. p_rect = one cell's centre + half-extents,
                // p_radii = the corner radius, p_host.xy = the period; the
                // host-box fade sides are pushed far out (a lattice never
                // fades against a host — its own rect bounds it).
                let (pw, ph) = (period.0.max(1e-3), period.1.max(1e-3));
                verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, [0.0; 4]));
                plate = Some(crate::draw::PlatePush {
                    rect: [origin.0 * scale, origin.1 * scale, cell.0 * 0.5 * scale, cell.1 * 0.5 * scale],
                    radii: [*radius * scale; 4],
                    light: [plate_light[0], plate_light[1], plate_light[2], *depth * scale],
                    material: plate_mat,
                    host: [pw * scale, ph * scale, 1e6, 1e6],
                    specular_tint: [1.0, 1.0, 1.0, 0.0],
                    mode: 13.0,
                    shape: crate::layout::corner_shape(),
                });
            }
            Prim::Grout { rect, period, origin, cell, radius, color } if shader_plates => {
                // The lattice's fold, painted flat (shader mode 15): one cover
                // quad in the grout colour; the shader keeps it outside the
                // cells. Same push layout as the lattice; light/material are
                // carried but unread.
                let (pw, ph) = (period.0.max(1e-3), period.1.max(1e-3));
                verts.extend(quad_vertices(rect.x, rect.y, rect.width, rect.height, sw, sh, *color));
                plate = Some(crate::draw::PlatePush {
                    rect: [origin.0 * scale, origin.1 * scale, cell.0 * 0.5 * scale, cell.1 * 0.5 * scale],
                    radii: [*radius * scale; 4],
                    light: [plate_light[0], plate_light[1], plate_light[2], 0.0],
                    material: plate_mat,
                    host: [pw * scale, ph * scale, 1e6, 1e6],
                    specular_tint: [1.0, 1.0, 1.0, 0.0],
                    mode: 15.0,
                    shape: crate::layout::corner_shape(),
                });
            }
            // Legacy banded path: no periodic wall — the lattice and the grout
            // draw nothing there, like the fillet (A/B comparison path only).
            Prim::Lattice { .. } | Prim::Grout { .. } => {}
            Prim::CarveUnion { boxes, depth, raised } if shader_plates => {
                // The union of several boxes as ONE wall (shader mode 14): the
                // boxes go into the frame's feature buffer as a contiguous run
                // and the shader takes the nearest one per pixel. The cover
                // quad is the union's bounding box grown by the wall's reach;
                // off-shape corners of it sit at the plateau and shade nothing.
                let budget = crate::draw::MAX_PLATE_FEATURES.saturating_sub(features.len());
                let take = boxes.len().min(budget);
                if take < boxes.len() && plate_debug() {
                    eprintln!(
                        "plate-carve: union of {} boxes gets {} — feature budget full ({} used)",
                        boxes.len(), take, features.len()
                    );
                }
                if take == 0 {
                    continue;
                }
                let kept = &boxes[..take];
                let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                for (r, _) in kept {
                    x0 = x0.min(r.x);
                    y0 = y0.min(r.y);
                    x1 = x1.max(r.x + r.width);
                    y1 = y1.max(r.y + r.height);
                }
                let infl = *depth * 0.5 + 2.0;
                verts.extend(quad_vertices(
                    x0 - infl, y0 - infl,
                    (x1 - x0) + 2.0 * infl, (y1 - y0) + 2.0 * infl,
                    sw, sh, [0.0; 4],
                ));
                let off = features.len() as f32;
                for (r, radii) in kept {
                    features.push([
                        (r.x + r.width * 0.5) * scale,
                        (r.y + r.height * 0.5) * scale,
                        r.width * 0.5 * scale,
                        r.height * 0.5 * scale,
                        radii.0 * scale,
                        radii.1 * scale,
                        radii.2 * scale,
                        radii.3 * scale,
                        *depth * scale,
                        0.0,
                        0.0,
                        0.0,
                    ]);
                }
                // The run is complete: a plate with an open feature run must
                // not append past it (its features would no longer be
                // contiguous), so it is closed here like any other appender.
                last_feature_plate = None;
                plate = Some(crate::draw::PlatePush {
                    rect: [
                        (x0 + x1) * 0.5 * scale,
                        (y0 + y1) * 0.5 * scale,
                        (x1 - x0) * 0.5 * scale,
                        (y1 - y0) * 0.5 * scale,
                    ],
                    // x: the raised flag; the shader reads nothing else here.
                    radii: [if *raised { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
                    light: [plate_light[0], plate_light[1], plate_light[2], *depth * scale],
                    material: plate_mat,
                    // Feature run [offset, count] (the renderer rebases the
                    // offset onto the frame slot, as for mode 1); zw far out
                    // so the host-box fade never applies.
                    host: [off, take as f32, 1e6, 1e6],
                    specular_tint: [1.0, 1.0, 1.0, 0.0],
                    mode: 14.0,
                    shape: crate::layout::corner_shape(),
                });
            }
            // Legacy banded path: no union — nothing is drawn there, like the
            // fillet and the lattice (A/B comparison path only).
            Prim::CarveUnion { .. } => {}
        }
        let end = verts.len() as u32;
        if end == start {
            continue;
        }
        // Some tessellators (quad_vertices, vector_vertices) don't thread the circle clip —
        // stamp the whole emitted range so every prim kind honors it uniformly.
        if item.clip_circle.is_some() {
            for v in verts[start as usize..].iter_mut() {
                v.clip_circle = no;
            }
        }
        // Merge into the previous batch if it shares this clip pair and is contiguous.
        // Plate batches carry per-draw push constants, and blur-behind batches
        // trigger the renderer's snapshot copy, so neither ever merges.
        if plate.is_none() && !blur_behind {
            // Ordinary geometry painted after a plate ends its carve-grouping
            // window: a recess emitted later must overlay this geometry (the
            // fallback path), not shade beneath it inside the plate's draw.
            if dbg_plates && !plate_stack.is_empty() {
                *dbg_closed_by.entry(prim_kind(&item.prim)).or_insert(0) += plate_stack.len();
            }
            plate_stack.clear();
            if let Some(last) = batches.last_mut() {
                if last.plate.is_none()
                    && last.scissor == item.clip
                    && last.clip_rrect == item.clip_rrect
                    && last.end == start
                {
                    last.end = end;
                    continue;
                }
            }
        }
        if promoted {
            plate_stack.clear();
        }
        batches.push(DlBatch { scissor: item.clip, clip_rrect: item.clip_rrect, start, end, plate, blur_behind });
        if let Some(prect) = made_plate {
            plate_stack.push((batches.len() - 1, prect));
            if dbg_plates {
                dbg_opened += 1;
            }
        }
    }

    if dbg_plates && (dbg_grouped > 0 || !dbg_fell_back.is_empty()) {
        eprintln!(
            "plate-dbg: {} carves — {dbg_grouped} grouped (exact CSG), {} overlay fallback",
            dbg_grouped + dbg_fell_back.len(),
            dbg_fell_back.len(),
        );
        eprintln!(
            "plate-dbg:   {dbg_opened} grouping window(s) opened by a filled plate; closed early by {}",
            if dbg_closed_by.is_empty() {
                "nothing".to_string()
            } else {
                dbg_closed_by
                    .iter()
                    .map(|(k, n)| format!("{k}x{n}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        );
        for line in &dbg_fell_back {
            eprintln!("plate-dbg: {line}");
        }
    }

    (verts, batches, images, features)
}

/// The push-constant block for a raised SDF-lit plate over `rect` (logical px in,
/// physical px out). Corner radii clamp to the half-extent cap the SDF needs.
///
/// `shape` is a per-plate corner exponent (`Prim::Plate`'s override); `None`
/// follows the DE-wide `layout::corner_shape`. The span factor follows the
/// exponent actually used, so a circular override (2.0) spans nothing and a
/// half-extent radius lands on a true circle.
#[allow(clippy::too_many_arguments)]
/// The push block of a frosted flat fill promoted to a zero-depth plate: a
/// mode-1 plate with no roll (`t` = 0.001, so the face is exactly the fill),
/// corners at the nominal radii in the configured `corner_shape`, and the
/// fill's own frost recipe in `host.zw` (`Material::from_fill` decodes the
/// sentinel).
///
/// The shape must be `corner_shape`, not a fixed circle: a frosted `Border`
/// draws its stroke as `push_plate_solid_border_vertices` geometry in that
/// shape, and the unfrosted fill fan uses it too. A circular face under a
/// squircle stroke left the stroke cutting inside the face's corners.
fn flat_frost_push(
    rect: &crate::scene::layout::Rect,
    radii: (f32, f32, f32, f32),
    fill: [f32; 4],
    scale: f32,
    light: [f32; 3],
    material: [f32; 4],
) -> crate::draw::PlatePush {
    let mut p = plate_push_raised(rect, radii, 0.0, scale, light, material, false, None);
    let [fz, fw] = crate::scene::material::Material::from_fill(fill).frost.pack(scale);
    p.host[2] = fz;
    p.host[3] = fw;
    p
}

fn plate_push_raised(
    rect: &crate::scene::layout::Rect,
    radii: (f32, f32, f32, f32),
    width: f32,
    scale: f32,
    light: [f32; 3],
    material: [f32; 4],
    scale_corners: bool,
    shape: Option<f32>,
) -> crate::draw::PlatePush {
    // Floored: a rect already shrunk past its padding (a window dragged
    // below what its layout can hold) has a NEGATIVE extent here, and
    // `clamp(0.0, cap)` with a negative cap is a panic, not a zero radius.
    let cap = (rect.width.min(rect.height) * 0.5).max(0.0);
    let shape = shape.map_or_else(crate::layout::corner_shape, |n| n.clamp(2.0, 16.0));
    // For PLATES (`scale_corners`), widen the corner span by the
    // curvature-match factor (see `layout::corner_span_factor`): the diagonal
    // curvature radius equals the configured radius, the corner reads as the
    // same size as a circular one, and every roll inset ≤ r stays crease-free
    // (past the diagonal curvature radius the offset curve the specular band
    // follows creases into a visible square corner). Widget-scale overlay
    // reliefs (recess/boss/ridge fallbacks) pass false: their radii must MATCH
    // the nominal-radius squircles of the widget silhouettes around them, and
    // at their few-px roll widths the offset crease is subpixel.
    let rscale = if scale_corners { crate::layout::corner_span_factor_for(shape) } else { 1.0 };
    crate::draw::PlatePush {
        rect: [
            (rect.x + rect.width * 0.5) * scale,
            (rect.y + rect.height * 0.5) * scale,
            rect.width * 0.5 * scale,
            rect.height * 0.5 * scale,
        ],
        radii: [
            (radii.0 * rscale).clamp(0.0, cap) * scale,
            (radii.1 * rscale).clamp(0.0, cap) * scale,
            (radii.2 * rscale).clamp(0.0, cap) * scale,
            (radii.3 * rscale).clamp(0.0, cap) * scale,
        ],
        light: [light[0], light[1], light[2], width * scale],
        material,
        // Mode-1 semantics: [feature offset, feature count] — no carves yet;
        // the tessellator fills these in as recesses group into this plate.
        host: [0.0, 0.0, 0.0, 0.0],
        specular_tint: [1.0, 1.0, 1.0, 0.0],
        mode: 1.0,
        shape,
    }
}

pub fn extra_quad_vertices(
    w: &dyn crate::widget::WidgetHost,
    qx: f32, qy: f32, qw: f32, qh: f32,
    sw: f32, sh: f32,
    qc: [f32; 4],
    clip_circle: [f32; 3],
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    push_extra_quad_vertices(w, qx, qy, qw, qh, sw, sh, qc, clip_circle, &mut verts);
    verts
}

fn get_child_widget_for_quad(
    w: &dyn crate::widget::WidgetHost,
    qx: f32, qy: f32, qw: f32, qh: f32,
) -> &dyn crate::widget::WidgetHost {
    if let Some(pbg) = w.as_any().downcast_ref::<crate::widget::ParametersBg>() {
        for s in pbg.sliders.iter().flatten() {
            let (sx, sy, sww, shh) = s.rect();
            if qx >= sx - 0.1 && qx + qw <= sx + sww + 0.1 && qy >= sy - 0.1 && qy + qh <= sy + shh + 0.1 {
                return s;
            }
        }
        for f in pbg.float3s.iter().flatten() {
            let (fx, fy, fww, fhh) = f.rect();
            if qx >= fx - 0.1 && qx + qw <= fx + fww + 0.1 && qy >= fy - 0.1 && qy + qh <= fy + fhh + 0.1 {
                return f;
            }
        }
        for sb in pbg.spinboxes.iter().flatten() {
            let (sx, sy, sww, shh) = sb.rect();
            if qx >= sx - 0.1 && qx + qw <= sx + sww + 0.1 && qy >= sy - 0.1 && qy + qh <= sy + shh + 0.1 {
                return sb;
            }
        }
        for btn in pbg.buttons.iter().flatten() {
            let (bx, by, bww, bhh) = btn.rect();
            if qx >= bx - 0.1 && qx + qw <= bx + bww + 0.1 && qy >= by - 0.1 && qy + qh <= by + bhh + 0.1 {
                return btn;
            }
        }
        for ch in pbg.choices.iter().flatten() {
            let (cx, cy, cww, chh) = ch.rect();
            if qx >= cx - 0.1 && qx + qw <= cx + cww + 0.1 && qy >= cy - 0.1 && qy + qh <= cy + chh + 0.1 {
                return ch;
            }
        }
        for t in pbg.texts.iter().flatten() {
            let (tx, ty, tww, thh) = t.rect();
            if qx >= tx - 0.1 && qx + qw <= tx + tww + 0.1 && qy >= ty - 0.1 && qy + qh <= ty + thh + 0.1 {
                return t;
            }
        }
        for cb in pbg.toggles.iter().flatten() {
            let (cx, cy, cww, chh) = cb.rect();
            if qx >= cx - 0.1 && qx + qw <= cx + cww + 0.1 && qy >= cy - 0.1 && qy + qh <= cy + chh + 0.1 {
                return cb;
            }
        }
        for c in pbg.colors.iter().flatten() {
            let (cx, cy, cww, chh) = c.rect();
            if qx >= cx - 0.1 && qx + qw <= cx + cww + 0.1 && qy >= cy - 0.1 && qy + qh <= cy + chh + 0.1 {
                return c;
            }
        }
    }
    w
}

pub fn push_extra_quad_vertices(
    w: &dyn crate::widget::WidgetHost,
    qx: f32, qy: f32, qw: f32, qh: f32,
    sw: f32, sh: f32,
    qc: [f32; 4],
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    if let Some(graph) = w.as_any().downcast_ref::<crate::widget::display::Graph>() {
        if graph.is_node_rect(qx, qy, qw, qh) {
            let r = crate::layout::graph_node_corner_radius();
            let extra_radii = crate::widget::CornerRadii::new(r, r, r, r);
            push_rounded_rect_vertices_corners(qx, qy, qw, qh, extra_radii, sw, sh, qc, clip_circle, None, out);
            return;
        }
    }

    let target_w = get_child_widget_for_quad(w, qx, qy, qw, qh);
    let radii = target_w.corner_radii();
    if radii.top_left <= 0.1 && radii.top_right <= 0.1 && radii.bottom_right <= 0.1 && radii.bottom_left <= 0.1 {
        out.extend_from_slice(&quad_vertices_with_clip(qx, qy, qw, qh, sw, sh, qc, clip_circle));
        if let Some((color, thickness)) = target_w.solid_border() {
            let (wx, wy, ww, wh) = target_w.rect();
            if (qx - wx).abs() < 0.1 && (qy - wy).abs() < 0.1 && (qw - ww).abs() < 0.1 && (qh - wh).abs() < 0.1 {
                push_plate_solid_border_vertices(qx, qy, qw, qh, radii, thickness, sw, sh, color, clip_circle, out);
            }
        }
        return;
    }

    let (wx, mut wy, ww, mut wh) = target_w.rect();
    let top_room = target_w.label_strip();
    wy += top_room;
    wh -= top_room;
    let extra_radii = crate::widget::CornerRadii::new(
        if qx <= wx + 1.5 && qy <= wy + 1.5 { radii.top_left } else { 0.0 },
        if qx + qw >= wx + ww - 1.5 && qy <= wy + 1.5 { radii.top_right } else { 0.0 },
        if qx + qw >= wx + ww - 1.5 && qy + qh >= wy + wh - 1.5 { radii.bottom_right } else { 0.0 },
        if qx <= wx + 1.5 && qy + qh >= wy + wh - 1.5 { radii.bottom_left } else { 0.0 },
    );

    push_rounded_rect_vertices_corners(qx, qy, qw, qh, extra_radii, sw, sh, qc, clip_circle, None, out);

    if let Some((color, thickness)) = target_w.solid_border() {
        let (rx, mut ry, rw, mut rh) = target_w.rect();
        let top = target_w.label_strip();
        ry += top;
        rh -= top;
        if (qx - rx).abs() < 0.1 && (qy - ry).abs() < 0.1 && (qw - rw).abs() < 0.1 && (qh - rh).abs() < 0.1 {
            push_plate_solid_border_vertices(qx, qy, qw, qh, radii, thickness, sw, sh, color, clip_circle, out);
        }
    }
}

pub fn extra_quad_vertices_clipped(
    w: &dyn crate::widget::WidgetHost,
    qx: f32, qy: f32, qw: f32, qh: f32,
    sw: f32, sh: f32,
    qc: [f32; 4],
    clip: (f32, f32, f32, f32),
    clip_circle: [f32; 3],
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    push_extra_quad_vertices_clipped(w, qx, qy, qw, qh, sw, sh, qc, clip, clip_circle, &mut verts);
    verts
}

pub fn push_extra_quad_vertices_clipped(
    w: &dyn crate::widget::WidgetHost,
    qx: f32, qy: f32, qw: f32, qh: f32,
    sw: f32, sh: f32,
    qc: [f32; 4],
    clip: (f32, f32, f32, f32),
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    if let Some(graph) = w.as_any().downcast_ref::<crate::widget::display::Graph>() {
        if graph.is_node_rect(qx, qy, qw, qh) {
            let r = crate::layout::graph_node_corner_radius();
            let extra_radii = crate::widget::CornerRadii::new(r, r, r, r);
            push_rounded_rect_vertices_corners(qx, qy, qw, qh, extra_radii, sw, sh, qc, clip_circle, Some(clip), out);
            return;
        }
    }

    let target_w = get_child_widget_for_quad(w, qx, qy, qw, qh);
    let radii = target_w.corner_radii();
    if radii.top_left <= 0.1 && radii.top_right <= 0.1 && radii.bottom_right <= 0.1 && radii.bottom_left <= 0.1 {
        let (cx0, cy0, cx1, cy1) = clip;
        let ix0 = qx.max(cx0);
        let iy0 = qy.max(cy0);
        let ix1 = (qx + qw).min(cx1);
        let iy1 = (qy + qh).min(cy1);
        if ix1 <= ix0 || iy1 <= iy0 {
            return;
        }
        out.extend_from_slice(&quad_vertices_with_clip(ix0, iy0, ix1 - ix0, iy1 - iy0, sw, sh, qc, clip_circle));
        if let Some((color, thickness)) = target_w.solid_border() {
            let (wx, wy, ww, wh) = target_w.rect();
            if (qx - wx).abs() < 0.1 && (qy - wy).abs() < 0.1 && (qw - ww).abs() < 0.1 && (qh - wh).abs() < 0.1 {
                push_plate_solid_border_vertices(qx, qy, qw, qh, radii, thickness, sw, sh, color, clip_circle, out);
            }
        }
        return;
    }

    let (wx, mut wy, ww, mut wh) = target_w.rect();
    let top_room = target_w.label_strip();
    wy += top_room;
    wh -= top_room;
    let extra_radii = crate::widget::CornerRadii::new(
        if qx <= wx + 1.5 && qy <= wy + 1.5 { radii.top_left } else { 0.0 },
        if qx + qw >= wx + ww - 1.5 && qy <= wy + 1.5 { radii.top_right } else { 0.0 },
        if qx + qw >= wx + ww - 1.5 && qy + qh >= wy + wh - 1.5 { radii.bottom_right } else { 0.0 },
        if qx <= wx + 1.5 && qy + qh >= wy + wh - 1.5 { radii.bottom_left } else { 0.0 },
    );

    push_rounded_rect_vertices_corners(qx, qy, qw, qh, extra_radii, sw, sh, qc, clip_circle, Some(clip), out);

    if let Some((color, thickness)) = target_w.solid_border() {
        let (rx, mut ry, rw, mut rh) = target_w.rect();
        let top = target_w.label_strip();
        ry += top;
        rh -= top;
        if (qx - rx).abs() < 0.1 && (qy - ry).abs() < 0.1 && (qw - rw).abs() < 0.1 && (qh - rh).abs() < 0.1 {
            push_plate_solid_border_vertices(qx, qy, qw, qh, radii, thickness, sw, sh, color, clip_circle, out);
        }
    }
}

pub fn circle_vertices(
    cx: f32, cy: f32, r: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    segments: usize,
    clip_circle: [f32; 3],
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    for i in 0..segments {
        let theta1 = (i as f32) * 2.0 * std::f32::consts::PI / (segments as f32);
        let theta2 = ((i + 1) as f32) * 2.0 * std::f32::consts::PI / (segments as f32);
        let x0 = cx;
        let y0 = cy;
        let x1 = cx + r * theta1.cos();
        let y1 = cy + r * theta1.sin();
        let x2 = cx + r * theta2.cos();
        let y2 = cy + r * theta2.sin();
        
        let ndc_x0 = (x0 / sw) * 2.0 - 1.0;
        let ndc_y0 = 1.0 - (y0 / sh) * 2.0;
        let ndc_x1 = (x1 / sw) * 2.0 - 1.0;
        let ndc_y1 = 1.0 - (y1 / sh) * 2.0;
        let ndc_x2 = (x2 / sw) * 2.0 - 1.0;
        let ndc_y2 = 1.0 - (y2 / sh) * 2.0;
        
        verts.push(Vertex { position: [ndc_x0, ndc_y0], color, clip_circle });
        verts.push(Vertex { position: [ndc_x1, ndc_y1], color, clip_circle });
        verts.push(Vertex { position: [ndc_x2, ndc_y2], color, clip_circle });
    }
    verts
}

pub fn circle_border_vertices(
    cx: f32, cy: f32, r: f32,
    thickness: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    segments: usize,
    clip_circle: [f32; 3],
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    for i in 0..segments {
        let theta1 = (i as f32) * 2.0 * std::f32::consts::PI / (segments as f32);
        let theta2 = ((i + 1) as f32) * 2.0 * std::f32::consts::PI / (segments as f32);
        
        let x0 = cx + (r - thickness) * theta1.cos();
        let y0 = cy + (r - thickness) * theta1.sin();
        let x1 = cx + r * theta1.cos();
        let y1 = cy + r * theta1.sin();
        
        let x2 = cx + r * theta2.cos();
        let y2 = cy + r * theta2.sin();
        let x3 = cx + (r - thickness) * theta2.cos();
        let y3 = cy + (r - thickness) * theta2.sin();
        
        let ndc_x0 = (x0 / sw) * 2.0 - 1.0; let ndc_y0 = 1.0 - (y0 / sh) * 2.0;
        let ndc_x1 = (x1 / sw) * 2.0 - 1.0; let ndc_y1 = 1.0 - (y1 / sh) * 2.0;
        let ndc_x2 = (x2 / sw) * 2.0 - 1.0; let ndc_y2 = 1.0 - (y2 / sh) * 2.0;
        let ndc_x3 = (x3 / sw) * 2.0 - 1.0; let ndc_y3 = 1.0 - (y3 / sh) * 2.0;
        
        verts.push(Vertex { position: [ndc_x0, ndc_y0], color, clip_circle });
        verts.push(Vertex { position: [ndc_x1, ndc_y1], color, clip_circle });
        verts.push(Vertex { position: [ndc_x2, ndc_y2], color, clip_circle });
        
        verts.push(Vertex { position: [ndc_x0, ndc_y0], color, clip_circle });
        verts.push(Vertex { position: [ndc_x2, ndc_y2], color, clip_circle });
        verts.push(Vertex { position: [ndc_x3, ndc_y3], color, clip_circle });
    }
    verts
}

pub fn arc_background_vertices(
    cx: f32, cy: f32, r: f32,
    thickness: f32,
    start_angle: f32, end_angle: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    segments: usize,
    clip_circle: [f32; 3],
) -> Vec<Vertex> {
    let mut verts = Vec::new();
    push_arc_background_vertices(cx, cy, r, thickness, start_angle, end_angle, sw, sh, color, segments, clip_circle, &mut verts);
    verts
}

/// A ring band with radial Gouraud shading: two sub-bands (inner rim → crest
/// centerline, crest → outer rim) whose vertex colors interpolate across the
/// stroke — the rounded-bevel profile — plus the half-px alpha feathers at
/// both true rims (colors matched to the adjacent band, so no seams).
#[allow(clippy::too_many_arguments)]
pub fn push_arc_shaded_vertices(
    cx: f32, cy: f32, r: f32,
    thickness: f32,
    start_angle: f32, end_angle: f32,
    sw: f32, sh: f32,
    inner: [f32; 4], crest: [f32; 4], outer: [f32; 4],
    segments: usize,
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    let f = 0.5f32.min(thickness * 0.25);
    let r_out = r;
    let r_in = (r - thickness).max(0.0);
    let r_mid = (r_in + r_out) / 2.0;
    let fade_in = [inner[0], inner[1], inner[2], 0.0];
    let fade_out = [outer[0], outer[1], outer[2], 0.0];
    // (inner radius, outer radius, color at inner edge, color at outer edge)
    let bands = [
        ((r_in - f).max(0.0), r_in + f, fade_in, inner),
        (r_in + f, r_mid, inner, crest),
        (r_mid, r_out - f, crest, outer),
        (r_out - f, r_out + f, outer, fade_out),
    ];
    for i in 0..segments {
        let theta1 = start_angle + (i as f32) * (end_angle - start_angle) / (segments as f32);
        let theta2 = start_angle + ((i + 1) as f32) * (end_angle - start_angle) / (segments as f32);
        let (c1, s1) = (theta1.cos(), theta1.sin());
        let (c2, s2) = (theta2.cos(), theta2.sin());
        for &(ra, rb, ca, cb) in &bands {
            if rb <= ra {
                continue;
            }
            let p = |rad: f32, c: f32, s: f32| -> [f32; 2] {
                [((cx + rad * c) / sw) * 2.0 - 1.0, 1.0 - ((cy + rad * s) / sh) * 2.0]
            };
            let (i1, o1) = (p(ra, c1, s1), p(rb, c1, s1));
            let (i2, o2) = (p(ra, c2, s2), p(rb, c2, s2));
            out.push(Vertex { position: i1, color: ca, clip_circle });
            out.push(Vertex { position: o1, color: cb, clip_circle });
            out.push(Vertex { position: o2, color: cb, clip_circle });
            out.push(Vertex { position: i1, color: ca, clip_circle });
            out.push(Vertex { position: o2, color: cb, clip_circle });
            out.push(Vertex { position: i2, color: ca, clip_circle });
        }
    }
}

pub fn push_arc_background_vertices(
    cx: f32, cy: f32, r: f32,
    thickness: f32,
    start_angle: f32, end_angle: f32,
    sw: f32, sh: f32,
    color: [f32; 4],
    segments: usize,
    clip_circle: [f32; 3],
    out: &mut Vec<Vertex>,
) {
    // The stroke band [r - thickness, r], with a half-px alpha ramp on each rim
    // (Gouraud across thin edge bands) so curved edges resolve smoothly instead
    // of hard-stepping — the poor-man's AA the flat pipeline doesn't provide.
    let f = 0.5f32.min(thickness * 0.25);
    let r_in = (r - thickness).max(0.0);
    // (inner radius, outer radius, alpha at inner rim, alpha at outer rim)
    let bands = [
        ((r_in - f).max(0.0), r_in + f, 0.0, color[3]),
        (r_in + f, r - f, color[3], color[3]),
        (r - f, r + f, color[3], 0.0),
    ];
    for i in 0..segments {
        let theta1 = start_angle + (i as f32) * (end_angle - start_angle) / (segments as f32);
        let theta2 = start_angle + ((i + 1) as f32) * (end_angle - start_angle) / (segments as f32);
        let (c1, s1) = (theta1.cos(), theta1.sin());
        let (c2, s2) = (theta2.cos(), theta2.sin());
        for &(ra, rb, aa, ab) in &bands {
            if rb <= ra {
                continue;
            }
            let ca = [color[0], color[1], color[2], aa];
            let cb = [color[0], color[1], color[2], ab];
            let p = |rad: f32, c: f32, s: f32| -> [f32; 2] {
                [((cx + rad * c) / sw) * 2.0 - 1.0, 1.0 - ((cy + rad * s) / sh) * 2.0]
            };
            let (i1, o1) = (p(ra, c1, s1), p(rb, c1, s1));
            let (i2, o2) = (p(ra, c2, s2), p(rb, c2, s2));
            out.push(Vertex { position: i1, color: ca, clip_circle });
            out.push(Vertex { position: o1, color: cb, clip_circle });
            out.push(Vertex { position: o2, color: cb, clip_circle });
            out.push(Vertex { position: i1, color: ca, clip_circle });
            out.push(Vertex { position: o2, color: cb, clip_circle });
            out.push(Vertex { position: i2, color: ca, clip_circle });
        }
    }
}

// `all(test, debug_assertions)`: the function under test only exists in
// debug builds, so a `cargo test --release` must compile the module out too.
#[cfg(all(test, debug_assertions))]
mod near_roll_fallback_tests {
    use super::near_roll_fallback_reason;
    use crate::scene::layout::Rect;

    fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, width: w, height: h }
    }

    const HOST: Rect = Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 };
    const ROLL: f32 = 8.0;

    #[test]
    fn interior_carve_is_quiet() {
        // Well inside the deflated host: the overlay fallback is exact there.
        let carve = r(100.0, 100.0, 200.0, 100.0);
        assert_eq!(near_roll_fallback_reason(&carve, 6.0, &HOST, ROLL, &[], false), None);
    }

    #[test]
    fn shaded_region_reaching_the_roll_is_loud() {
        // Carve rect stops 3px short of the roll band, but its shaded region
        // (depth*0.5 + 2 = 5px) crosses in — the inflation must count.
        let carve = r(ROLL + 3.0, 100.0, 200.0, 100.0);
        assert_eq!(
            near_roll_fallback_reason(&carve, 6.0, &HOST, ROLL, &[], false),
            Some("the host's feature run is closed (another plate appended features since)")
        );
    }

    #[test]
    fn occlusion_is_named_before_run_contiguity() {
        let carve = r(2.0, 100.0, 200.0, 100.0);
        let occluder = r(150.0, 150.0, 100.0, 100.0);
        assert_eq!(
            near_roll_fallback_reason(&carve, 6.0, &HOST, ROLL, &[occluder], false),
            Some("a later plate overlaps the carve's shaded region")
        );
    }

    #[test]
    fn non_overlapping_later_plate_is_not_occlusion() {
        let carve = r(2.0, 100.0, 200.0, 100.0);
        let elsewhere = r(500.0, 400.0, 100.0, 100.0);
        assert_eq!(
            near_roll_fallback_reason(&carve, 6.0, &HOST, ROLL, &[elsewhere], false),
            Some("the host's feature run is closed (another plate appended features since)")
        );
    }

    #[test]
    fn budget_wins_over_every_other_reason() {
        let carve = r(2.0, 100.0, 200.0, 100.0);
        let occluder = r(150.0, 150.0, 100.0, 100.0);
        assert_eq!(
            near_roll_fallback_reason(&carve, 6.0, &HOST, ROLL, &[occluder], true),
            Some("the feature budget is full")
        );
    }
}

#[cfg(test)]
mod frame_tests {
    use crate::scene::layout::Rect;
    use crate::scene::paint::PaintCtx;

    /// A frame is a plate turned inside out: its batch carries the HOLE as
    /// the SDF box, in mode 17, its cover quad is the face's bound, and it
    /// hosts the carves inside that bound as a Bevel does.
    #[test]
    fn a_frame_is_an_inside_out_plate_that_hosts_its_carves() {
        if !crate::layout::bevel_shader() {
            eprintln!("skipping: the shader plates are off in this configuration");
            return;
        }
        let (w, h, scale) = (400.0f32, 200.0f32, 1.0f32);
        let face = Rect { x: -10.0, y: 100.0, width: 420.0, height: 110.0 };
        let hole = Rect { x: 0.0, y: -100.0, width: 400.0, height: 224.0 };
        let mut pc = PaintCtx::new();
        let material = crate::scene::Material::opaque([0.3, 0.3, 0.35, 1.0]);
        pc.frame(face, hole, (0.0, 0.0, 20.0, 20.0), &material, 8.0);
        pc.recess(Rect { x: 40.0, y: 150.0, width: 30.0, height: 20.0 }, (4.0, 4.0, 4.0, 4.0), 3.0);
        let dl = pc.finish();
        let (_, batches, _, features) = super::tessellate_display_list(&dl, w, h, scale);
        let plate = batches.iter().find_map(|b| b.plate.filter(|p| p.mode == 17.0)).expect("a mode-17 batch");
        assert_eq!(plate.rect, [200.0, 12.0, 200.0, 112.0], "the SDF box is the hole");
        assert_eq!(plate.radii[2], 20.0, "the hole's bottom corners are the coves");
        assert_eq!(features.len(), 1, "the carve inside the face groups into the frame");
    }
}
