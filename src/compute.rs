//! What a compute job is, apart from the device that runs it: a [`Kernel`]
//! (WGSL source and an entry point), its [`Binding`]s in `@binding(i)` order,
//! and the rules a job is held to before anything reaches a GPU — the
//! binding count, the dispatch, the ping-pong pair, the kernel parsed and
//! validated by naga with its own diagnostics.
//!
//! Two devices run jobs: `vk::ComputeDevice` (a headless Vulkan device,
//! synchronous) and, in the browser, `web::ComputeDevice` (WebGPU, whose
//! readback is a promise, so its `run` is async). Both take these types and
//! answer a job the same way, so a kernel and its bindings are written once.

/// A compute shader: WGSL source and the `@compute` entry point to run.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Kernel {
    pub source: String,
    pub entry: String,
}

impl Kernel {
    pub fn new(source: impl Into<String>, entry: impl Into<String>) -> Self {
        Kernel { source: source.into(), entry: entry.into() }
    }
}

/// How a binding is declared to the shader, in `@binding(i)` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BindKind {
    /// `var<storage, read>` or `var<storage, read_write>`.
    Storage,
    /// `var<uniform>`: a small parameter block, 16-byte layout rules apply.
    Uniform,
}

/// One buffer of a job, bound at `@group(0) @binding(i)` for its index in
/// the list handed to a device's `run`.
pub enum Binding<'a> {
    /// Read-write storage: uploaded before the dispatch and READ BACK into
    /// the same slice after it.
    Storage(&'a mut [u8]),
    /// Read-only storage: uploaded, never read back.
    Input(&'a [u8]),
    /// A uniform block: uploaded, never read back.
    Uniform(&'a [u8]),
}

impl<'a> Binding<'a> {
    /// A read-write binding over a typed slice (`&mut [f32]`, `&mut [[f32; 3]]`, …).
    pub fn rw<T: bytemuck::Pod>(data: &'a mut [T]) -> Self {
        Binding::Storage(bytemuck::cast_slice_mut(data))
    }

    /// A read-only storage binding over a typed slice.
    pub fn input<T: bytemuck::Pod>(data: &'a [T]) -> Self {
        Binding::Input(bytemuck::cast_slice(data))
    }

    /// A uniform binding over one `Pod` struct.
    pub fn uniform<T: bytemuck::Pod>(value: &'a T) -> Self {
        Binding::Uniform(bytemuck::bytes_of(value))
    }

    pub(crate) fn kind(&self) -> BindKind {
        match self {
            Binding::Storage(_) | Binding::Input(_) => BindKind::Storage,
            Binding::Uniform(_) => BindKind::Uniform,
        }
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        match self {
            Binding::Storage(b) => b,
            Binding::Input(b) => b,
            Binding::Uniform(b) => b,
        }
    }
}

/// Workgroups needed to cover `items` at `per_group` invocations each — the
/// `@workgroup_size` of the entry point, which a device's `run_over` reads
/// for you.
pub fn workgroups(items: u32, per_group: u32) -> u32 {
    items.div_ceil(per_group.max(1)).max(1)
}

/// The most bindings one job may carry.
pub const MAX_BINDINGS: usize = 16;

/// Storage bindings are bound whole, so a buffer's size has to be a multiple
/// of the widest element stride a shader may declare; 16 covers `vec4<f32>`.
pub(crate) const BUFFER_ALIGN: usize = 16;

/// A binding of `len` bytes as the buffer it is uploaded into: at least one
/// alignment unit, rounded up to one. The padding is uploaded as zeros.
pub(crate) fn padded_len(len: usize) -> usize {
    len.max(BUFFER_ALIGN).div_ceil(BUFFER_ALIGN) * BUFFER_ALIGN
}

/// Hold a job to the rules before any device is asked: the binding count,
/// a dispatch with no zero in it, at least one pass, and a ping-pong pair
/// that names a read-only input and a read-write output of one length.
pub(crate) fn check_job(
    bindings: &[Binding<'_>],
    groups: [u32; 3],
    passes: u32,
    ping_pong: Option<(usize, usize)>,
) -> Result<(), String> {
    if bindings.len() > MAX_BINDINGS {
        return Err(format!("{} bindings; a job may carry at most {MAX_BINDINGS}", bindings.len()));
    }
    if groups.iter().any(|&g| g == 0) {
        return Err(format!("workgroup count {groups:?} has a zero"));
    }
    if passes == 0 {
        return Err("a job needs at least one pass".to_string());
    }
    if let Some((a, b)) = ping_pong {
        if a == b || a >= bindings.len() || b >= bindings.len() {
            return Err(format!("ping-pong pair ({a}, {b}) does not name two distinct bindings of {}", bindings.len()));
        }
        if !matches!(bindings[a], Binding::Input(_)) {
            return Err(format!("ping-pong binding {a} must be a read-only Input: it is where the first pass reads"));
        }
        if !matches!(bindings[b], Binding::Storage(_)) {
            return Err(format!("ping-pong binding {b} must be a read-write Storage: it is where the result lands"));
        }
        if bindings[a].bytes().len() != bindings[b].bytes().len() {
            return Err(format!(
                "ping-pong bindings {a} and {b} differ in length ({} vs {} bytes)",
                bindings[a].bytes().len(),
                bindings[b].bytes().len()
            ));
        }
    }
    Ok(())
}

/// The buffer bound at `binding` in a pass: a ping-pong pass with `swapped`
/// set (every second one) binds the pair's two buffers the other way round.
pub(crate) fn slot_for(binding: usize, swapped: bool, ping_pong: Option<(usize, usize)>) -> usize {
    match ping_pong {
        Some((a, b)) if swapped && binding == a => b,
        Some((a, b)) if swapped && binding == b => a,
        _ => binding,
    }
}

/// The buffer a read-write binding is read back from: the ping-pong output
/// from whichever buffer the LAST pass wrote, every other one from its own.
pub(crate) fn result_slot(binding: usize, passes: u32, ping_pong: Option<(usize, usize)>) -> usize {
    match ping_pong {
        Some((a, b)) if binding == b && passes % 2 == 0 => a,
        _ => binding,
    }
}

/// A kernel parsed and validated, with what a device needs to know of it.
pub(crate) struct ParsedKernel {
    pub module: naga::Module,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub info: naga::valid::ModuleInfo,
    pub workgroup_size: [u32; 3],
}

impl ParsedKernel {
    /// Whether the module declares `@group(0) @binding(i)` as read-only
    /// storage (`var<storage, read>`). WebGPU's layouts tell read-only from
    /// read-write storage, where Vulkan's do not.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn read_only_storage(&self, binding: u32) -> bool {
        self.module.global_variables.iter().any(|(_, g)| {
            g.binding.as_ref().is_some_and(|b| b.group == 0 && b.binding == binding)
                && matches!(g.space, naga::AddressSpace::Storage { access } if !access.contains(naga::StorageAccess::STORE))
        })
    }
}

/// Parse and validate a kernel, every failure reported with naga's own
/// diagnostic — a kernel may be a user's, so a bad one is an `Err`, never a
/// panic — and find its `@compute` entry point, naming what the module does
/// offer when there is none of that name.
pub(crate) fn parse_kernel(kernel: &Kernel) -> Result<ParsedKernel, String> {
    let module = naga::front::wgsl::parse_str(&kernel.source)
        .map_err(|e| format!("WGSL parse error: {}", e.emit_to_string(&kernel.source).trim_end()))?;
    let entry = module
        .entry_points
        .iter()
        .find(|ep| ep.name == kernel.entry && ep.stage == naga::ShaderStage::Compute)
        .ok_or_else(|| {
            let offered: Vec<&str> = module
                .entry_points
                .iter()
                .filter(|ep| ep.stage == naga::ShaderStage::Compute)
                .map(|ep| ep.name.as_str())
                .collect();
            format!(
                "no @compute entry point named `{}`; the module offers {}",
                kernel.entry,
                if offered.is_empty() { "none".to_string() } else { offered.join(", ") }
            )
        })?;
    let workgroup_size = entry.workgroup_size;
    let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
        .validate(&module)
        .map_err(|e| format!("WGSL validation error: {}", e.emit_to_string(&kernel.source).trim_end()))?;
    Ok(ParsedKernel { module, info, workgroup_size })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_job_is_held_to_the_rules_before_a_device_sees_it() {
        let mut out = [0.0f32; 4];
        let inp = [0.0f32; 4];
        let short = [0.0f32; 2];
        assert!(check_job(&[Binding::rw(&mut out)], [1, 1, 1], 1, None).is_ok());
        assert!(check_job(&[Binding::rw(&mut out)], [1, 0, 1], 1, None).unwrap_err().contains("zero"));
        assert!(check_job(&[Binding::rw(&mut out)], [1, 1, 1], 0, None).unwrap_err().contains("pass"));
        assert!(check_job(&[Binding::input(&inp), Binding::rw(&mut out)], [1, 1, 1], 2, Some((0, 1))).is_ok());
        assert!(check_job(&[Binding::input(&inp), Binding::rw(&mut out)], [1, 1, 1], 2, Some((1, 0)))
            .unwrap_err()
            .contains("read-only Input"));
        assert!(check_job(&[Binding::input(&short), Binding::rw(&mut out)], [1, 1, 1], 2, Some((0, 1)))
            .unwrap_err()
            .contains("differ in length"));
    }

    #[test]
    fn the_ping_pong_result_is_wherever_the_last_pass_wrote() {
        let pp = Some((0, 1));
        assert_eq!((slot_for(0, true, pp), slot_for(1, true, pp), slot_for(2, true, pp)), (1, 0, 2));
        assert_eq!(slot_for(0, false, pp), 0);
        assert_eq!(result_slot(1, 3, pp), 1);
        assert_eq!(result_slot(1, 4, pp), 0);
        assert_eq!(result_slot(1, 4, None), 1);
    }

    #[test]
    fn a_kernel_reports_its_workgroup_size_its_read_only_bindings_and_its_errors() {
        let src = "@group(0) @binding(0) var<storage, read> a: array<f32>;
                   @group(0) @binding(1) var<storage, read_write> b: array<f32>;
                   @compute @workgroup_size(64) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
                       if id.x < arrayLength(&b) { b[id.x] = a[id.x] * 2.0; }
                   }";
        let k = parse_kernel(&Kernel::new(src, "main")).unwrap();
        assert_eq!(k.workgroup_size, [64, 1, 1]);
        assert!(k.read_only_storage(0));
        assert!(!k.read_only_storage(1));
        assert!(parse_kernel(&Kernel::new(src, "nope")).err().unwrap().contains("offers main"));
        assert!(parse_kernel(&Kernel::new("fn (", "main")).err().unwrap().contains("parse error"));
    }
}
