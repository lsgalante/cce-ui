// rt_denoise.wgsl — edge-avoiding à-trous wavelet denoiser (Dammertz et al.)
// for the path tracer's progressive output.
//
// Runs as a few compute iterations after each trace dispatch: a 5×5 B3-spline
// kernel dilated by `step` (1, 2, 4, …), with edge-stopping weights from the
// tracer's primary-hit features (normal + depth) and color similarity. The
// color sigma scales with 1/sqrt(sample count), so the filter is strong on
// 1-spp camera-drag frames and fades toward identity as the accumulation
// converges — no pop when it effectively "turns off".
//
// Iteration wiring (single pipeline, per-iteration dynamic uniform):
//   first != 0 — source is the accumulation mean (accum.rgb / accum.a);
//                otherwise the `src` ping-pong buffer.
//   last  != 0 — the result is also tone-mapped into `out_img` (replacing
//                the tracer's own write); it is always written to `dst`.

struct DenoiseParams {
    width: u32,
    height: u32,
    step: u32,
    first: u32,
    last: u32,
    // 1/sqrt(accumulated samples): scales the color edge-stopping sigma.
    inv_sqrt_n: f32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0) var<uniform> dp: DenoiseParams;
@group(0) @binding(1) var<storage, read> accum: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> features: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(5) var out_img: texture_storage_2d<rgba8unorm, write>;

fn load_color(idx: u32) -> vec3<f32> {
    if dp.first != 0u {
        let a = accum[idx];
        return a.rgb / max(a.a, 1.0);
    }
    return src[idx].rgb;
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// B3 spline: center, ±1, ±2.
fn kernel_w(k: i32) -> f32 {
    if k == 0 {
        return 0.375;
    }
    if k == 1 {
        return 0.25;
    }
    return 0.0625;
}

@compute @workgroup_size(8, 8)
fn cs_denoise(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= dp.width || gid.y >= dp.height {
        return;
    }
    let idx = gid.y * dp.width + gid.x;
    let center_c = load_color(idx);
    let center_f = features[2u * idx];
    let center_n = center_f.xyz;
    let center_t = center_f.w;
    let center_l = luminance(center_c);

    // Sky pixels (t = 1e30) are already noise-free; pass them through rather
    // than letting the huge depth deltas produce all-zero weights.
    if center_t >= 1e30 {
        dst[idx] = vec4<f32>(center_c, 1.0);
        if dp.last != 0u {
            textureStore(
                out_img,
                vec2<i32>(i32(gid.x), i32(gid.y)),
                vec4<f32>(clamp(center_c, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0),
            );
        }
        return;
    }

    let sigma_c = max(0.85 * dp.inv_sqrt_n, 1e-3);
    let sigma_n = 0.15;
    let sigma_t = 0.10 * max(center_t, 0.1);

    var sum = vec3<f32>(0.0);
    var weight_sum = 0.0;
    for (var dy: i32 = -2; dy <= 2; dy = dy + 1) {
        for (var dx: i32 = -2; dx <= 2; dx = dx + 1) {
            let x = i32(gid.x) + dx * i32(dp.step);
            let y = i32(gid.y) + dy * i32(dp.step);
            if x < 0 || y < 0 || x >= i32(dp.width) || y >= i32(dp.height) {
                continue;
            }
            let j = u32(y) * dp.width + u32(x);
            let f = features[2u * j];
            if f.w >= 1e30 {
                continue; // never mix sky into surfaces
            }
            let c = load_color(j);

            let dn = center_n - f.xyz;
            let w_n = exp(-dot(dn, dn) / (sigma_n * sigma_n));
            let dt = (center_t - f.w) / sigma_t;
            let w_t = exp(-dt * dt);
            let dl = (center_l - luminance(c)) / sigma_c;
            let w_c = exp(-dl * dl);
            let w = kernel_w(abs(dx)) * kernel_w(abs(dy)) * w_n * w_t * w_c;

            sum = sum + c * w;
            weight_sum = weight_sum + w;
        }
    }
    let result = sum / max(weight_sum, 1e-6);
    dst[idx] = vec4<f32>(result, 1.0);
    if dp.last != 0u {
        textureStore(
            out_img,
            vec2<i32>(i32(gid.x), i32(gid.y)),
            vec4<f32>(clamp(result, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0),
        );
    }
}
