//! The compute probe's jobs, shared by both halves: each runs on a device
//! and is held to a CPU reference, and prints a digest of its result bytes,
//! so the native run (`compute_native`, Vulkan) and the browser's
//! (`compute_web`, WebGPU) can be compared line for line. The arithmetic is
//! exact in f32 (small integers, power-of-two weights), so a conformant
//! device must match the reference — and the other device — to the bit.

use cce_ui::compute::{Binding, Kernel};

pub const DOUBLE: &str = "
@group(0) @binding(0) var<storage, read> a: array<f32>;
@group(0) @binding(1) var<storage, read_write> b: array<f32>;
@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x < arrayLength(&b) { b[id.x] = a[id.x] * 2.0; }
}";

pub const SAXPY: &str = "
struct P { a: f32, n: u32 }
@group(0) @binding(0) var<uniform> p: P;
@group(0) @binding(1) var<storage, read> x: array<f32>;
@group(0) @binding(2) var<storage, read_write> y: array<f32>;
@compute @workgroup_size(32) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x < p.n { y[id.x] = p.a * x[id.x] + y[id.x]; }
}";

pub const BLUR: &str = "
@group(0) @binding(0) var<storage, read> src: array<f32>;
@group(0) @binding(1) var<storage, read_write> dst: array<f32>;
@compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let n = arrayLength(&dst);
    let i = id.x;
    if i >= n { return; }
    let l = src[max(i, 1u) - 1u];
    let r = src[min(i + 1u, n - 1u)];
    dst[i] = 0.25 * l + 0.5 * src[i] + 0.25 * r;
}";

pub const GRID: &str = "
@group(0) @binding(0) var<storage, read_write> g: array<f32>;
@compute @workgroup_size(8, 8) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x < 32u && id.y < 24u { g[id.y * 32u + id.x] = f32(id.x) + 1000.0 * f32(id.y); }
}";

/// A kernel over `n` bindings: `n - 1` inputs summed into the last.
pub fn many_source(n: usize) -> String {
    let mut s = String::new();
    for i in 0..n - 1 {
        s += &format!("@group(0) @binding({i}) var<storage, read> in{i}: array<f32>;\n");
    }
    s += &format!("@group(0) @binding({}) var<storage, read_write> out: array<f32>;\n", n - 1);
    s += "@compute @workgroup_size(16) fn main(@builtin(global_invocation_id) id: vec3<u32>) {\n";
    s += "    let i = id.x; if i >= arrayLength(&out) { return; }\n    var t = 0.0;\n";
    for i in 0..n - 1 {
        s += &format!("    t += in{i}[i];\n");
    }
    s += "    out[i] = t;\n}\n";
    s
}

/// FNV-1a over a result's bytes: what two devices are compared by.
pub fn digest(v: &[f32]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytemuck::cast_slice::<f32, u8>(v) {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

pub fn report(name: &str, got: &[f32], want: &[f32]) -> String {
    let worst = got.iter().zip(want).map(|(g, w)| (g - w).abs()).fold(0.0f32, f32::max);
    let ok = got.len() == want.len() && got.iter().zip(want).all(|(g, w)| g.to_bits() == w.to_bits());
    format!("{name}: {} n={} max|d|={worst} digest={}", if ok { "exact" } else { "DIFFERS" }, got.len(), digest(got))
}

/// The jobs, written once over whichever device runs them. A macro rather
/// than a generic function: one device's `run` is synchronous and the
/// other's async, and `$await` is the one word between them.
#[macro_export]
macro_rules! compute_jobs {
    ($dev:expr, $($await:tt)*) => {{
        use cce_ui::compute::{Binding, Kernel};
        use jobs::*;
        let mut lines: Vec<String> = Vec::new();

        // A map over 1000 elements, sized from the entry's @workgroup_size.
        let a: Vec<f32> = (0..1000).map(|i| i as f32 - 500.0).collect();
        let mut b = vec![0.0f32; 1000];
        let r = $dev.run_over(&Kernel::new(DOUBLE, "main"), &mut [Binding::input(&a), Binding::rw(&mut b)], 1000)$($await)*;
        let want: Vec<f32> = a.iter().map(|v| v * 2.0).collect();
        lines.push(match r { Ok(()) => report("double", &b, &want), Err(e) => format!("double: ERR {e}") });

        // A uniform block beside storage.
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct P { a: f32, n: u32, _pad: [u32; 2] }
        let x: Vec<f32> = (0..777).map(|i| (i % 13) as f32).collect();
        let mut y: Vec<f32> = (0..777).map(|i| (i % 7) as f32).collect();
        let want: Vec<f32> = x.iter().zip(&y).map(|(x, y)| 2.5 * x + y).collect();
        let p = P { a: 2.5, n: 777, _pad: [0; 2] };
        let r = $dev.run_over(&Kernel::new(SAXPY, "main"), &mut [Binding::uniform(&p), Binding::input(&x), Binding::rw(&mut y)], 777)$($await)*;
        lines.push(match r { Ok(()) => report("saxpy", &y, &want), Err(e) => format!("saxpy: ERR {e}") });

        // Ping-pong passes, odd and even counts: the result lands in the
        // output binding either way. 4096 elements: a length the 16-byte
        // padding leaves alone, since `arrayLength` counts the padding.
        for passes in [33u32, 34] {
            let src: Vec<f32> = (0..4096).map(|i| if i % 512 == 256 { 4096.0 } else { 0.0 }).collect();
            let mut dst = vec![0.0f32; 4096];
            let mut want = src.clone();
            for _ in 0..passes {
                let n = want.len();
                want = (0..n).map(|i| 0.25 * want[i.saturating_sub(1)] + 0.5 * want[i] + 0.25 * want[(i + 1).min(n - 1)]).collect();
            }
            let r = $dev
                .run_passes_over(&Kernel::new(BLUR, "main"), &mut [Binding::input(&src), Binding::rw(&mut dst)], 4096, passes, Some((0, 1)))
                $($await)*;
            lines.push(match r { Ok(()) => report(&format!("blur x{passes}"), &dst, &want), Err(e) => format!("blur x{passes}: ERR {e}") });
        }

        // A 2D dispatch.
        let mut g = vec![-1.0f32; 32 * 24];
        let r = $dev.run(&Kernel::new(GRID, "main"), &mut [Binding::rw(&mut g)], [4, 3, 1])$($await)*;
        let want: Vec<f32> = (0..32 * 24).map(|i| (i % 32) as f32 + 1000.0 * (i / 32) as f32).collect();
        lines.push(match r { Ok(()) => report("grid", &g, &want), Err(e) => format!("grid: ERR {e}") });

        // Ten bindings: past WebGPU's default of eight storage buffers a
        // stage (the browser device asks the adapter for its own limit),
        // within what every adapter offers — SwiftShader's is ten.
        let ins: Vec<Vec<f32>> = (0..9).map(|k| (0..100).map(|i| (k * 100 + i) as f32).collect()).collect();
        let mut out = vec![0.0f32; 100];
        let want: Vec<f32> = (0..100).map(|i| ins.iter().map(|v| v[i]).sum()).collect();
        let mut binds: Vec<Binding> = ins.iter().map(|v| Binding::input(v)).collect();
        binds.push(Binding::rw(&mut out));
        let src = many_source(10);
        let r = $dev.run_over(&Kernel::new(src.as_str(), "main"), &mut binds, 100)$($await)*;
        drop(binds);
        lines.push(match r { Ok(()) => report("ten bindings", &out, &want), Err(e) => format!("ten bindings: ERR {e}") });

        // A user's bad kernel is an Err, with naga's word for what is wrong.
        let mut z = vec![0.0f32; 4];
        let r = $dev.run(&Kernel::new("fn main( {", "main"), &mut [Binding::rw(&mut z)], [1, 1, 1])$($await)*;
        lines.push(format!("bad wgsl: {}", match r { Err(e) if e.contains("parse error") => "Err(parse error)".to_string(), other => format!("{:?}", other) }));
        let r = $dev.run(&Kernel::new(DOUBLE, "nope"), &mut [Binding::input(&a), Binding::rw(&mut z)], [1, 1, 1])$($await)*;
        lines.push(format!("no entry: {}", match r { Err(e) => e, Ok(()) => "Ok".into() }));
        lines
    }};
}

#[allow(dead_code)]
fn _uses(_: Binding, _: Kernel) {}
