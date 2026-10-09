//! Vertex builders for flat shapes: quads (plain, clipped, shaded), lines and capped
//! vectors, rounded rects with the DE's superellipse corner, glow, circles and arcs.

use super::*;

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
pub(super) fn push_feathered_line_vertices(
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
pub(super) fn superellipse_pt(theta: f32, e: f32) -> (f32, f32) {
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
