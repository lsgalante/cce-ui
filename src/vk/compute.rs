//! The compute-job API: upload buffers, dispatch a WGSL kernel, read back.
//!
//! The renderer already had everything a compute job needs — naga compiles
//! WGSL to SPIR-V at runtime, the path tracer builds compute pipelines over
//! storage buffers, and `RtOffscreen` runs a headless device with no window
//! — but all of it was internal to the RT pass and read back only an image.
//! This module is that machinery with a general face: a [`ComputeDevice`]
//! owns a headless [`VkCore`], and [`ComputeDevice::run`] takes a
//! [`Kernel`] and a list of [`Binding`]s, uploads them, dispatches, waits,
//! and copies every read-write binding back into the caller's slice.
//!
//! Designed for cce-designer's Phase 7 step 4 (see its `shapeshifter.md`):
//! the per-point solver operators — relax, diffuse, collide — written once
//! in WGSL over the columnar attribute arrays a `Detail` already keeps, with
//! the CPU evaluator as the reference each is held to. The shape of the API
//! follows from that use: the caller has arrays in memory and wants them
//! transformed, so buffers are HOST-VISIBLE and mapped, upload and readback
//! are memcpys through the mapping, and there is no staging copy. On an
//! integrated GPU that is the fastest path there is; on a discrete one it is
//! correct and simple, and a device-local tier can be added behind the same
//! API if a workload ever asks for it.
//!
//! Every failure is an `Err(String)`, never a panic, because a kernel may be
//! user-authored: a WGSL error comes back with naga's own diagnostic, a
//! missing entry point names what the module does offer, and a device
//! without Vulkan reports as such from [`ComputeDevice::new`].
//!
//! Not `Send`: it owns a device and a command buffer. Make one per thread
//! that computes, and keep it — pipelines cache by source and entry point,
//! and buffers are reused across runs when they fit.

use super::core::VkCore;
use super::renderer::{create_cpu_buffer, destroy_cpu_buffer, AllocatedBuffer};
use ash::vk;
use std::collections::HashMap;
use std::ffi::CString;

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
/// the list handed to [`ComputeDevice::run`].
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

    fn kind(&self) -> BindKind {
        match self {
            Binding::Storage(_) | Binding::Input(_) => BindKind::Storage,
            Binding::Uniform(_) => BindKind::Uniform,
        }
    }

    fn bytes(&self) -> &[u8] {
        match self {
            Binding::Storage(b) => b,
            Binding::Input(b) => b,
            Binding::Uniform(b) => b,
        }
    }
}

/// Workgroups needed to cover `items` at `per_group` invocations each — the
/// `@workgroup_size` of the entry point, which [`ComputeDevice::run_over`]
/// reads for you.
pub fn workgroups(items: u32, per_group: u32) -> u32 {
    items.div_ceil(per_group.max(1)).max(1)
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct PipelineKey {
    kernel: Kernel,
    kinds: Vec<BindKind>,
}

struct Pipeline {
    set_layout: vk::DescriptorSetLayout,
    layout: vk::PipelineLayout,
    module: vk::ShaderModule,
    pipeline: vk::Pipeline,
    workgroup_size: [u32; 3],
}

/// Storage bindings are bound whole, so a buffer's size has to be a multiple
/// of the widest element stride a shader may declare; 16 covers `vec4<f32>`.
const BUFFER_ALIGN: usize = 16;

/// A headless device that runs compute jobs. See the module docs.
pub struct ComputeDevice {
    pipelines: HashMap<PipelineKey, Pipeline>,
    /// One buffer per binding index, grown when a job needs more room.
    slots: Vec<AllocatedBuffer>,
    descriptor_pool: vk::DescriptorPool,
    cmd: vk::CommandBuffer,
    fence: vk::Fence,
    /// Declared last: everything above is destroyed before the device.
    core: VkCore,
}

/// The most bindings one job may carry (the descriptor pool is sized to it).
pub const MAX_BINDINGS: usize = 16;

impl ComputeDevice {
    /// A device on the machine's preferred GPU (`CCE_VK_DEVICE` steers it,
    /// as for every renderer). `Err` when there is no usable Vulkan at all.
    pub fn new() -> Result<Self, String> {
        // VkCore reports an absent driver by panicking, as a renderer with no
        // window to draw into has nothing better to do; a compute consumer
        // has a CPU path to fall back to, so the panic is caught here.
        let core = std::panic::catch_unwind(VkCore::new_headless).map_err(|e| {
            let msg = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown".to_string());
            format!("no Vulkan compute device: {msg}")
        })?;
        let device = core.device.clone();
        unsafe {
            let cmd = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(core.command_pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )
                .map_err(|e| format!("command buffer: {e}"))?[0];
            let fence = device
                .create_fence(&vk::FenceCreateInfo::default(), None)
                .map_err(|e| format!("fence: {e}"))?;
            let pool_sizes = [
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(2 * MAX_BINDINGS as u32),
                vk::DescriptorPoolSize::default()
                    .ty(vk::DescriptorType::UNIFORM_BUFFER)
                    .descriptor_count(2 * MAX_BINDINGS as u32),
            ];
            let descriptor_pool = device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default().max_sets(2).pool_sizes(&pool_sizes),
                    None,
                )
                .map_err(|e| format!("descriptor pool: {e}"))?;
            Ok(ComputeDevice {
                pipelines: HashMap::new(),
                slots: Vec::new(),
                descriptor_pool,
                cmd,
                fence,
                core,
            })
        }
    }

    /// The physical device's name, for a log line or a status readout.
    pub fn device_name(&self) -> String {
        unsafe {
            let props = self.core.instance.get_physical_device_properties(self.core.physical_device);
            std::ffi::CStr::from_ptr(props.device_name.as_ptr()).to_string_lossy().into_owned()
        }
    }

    /// The entry point's `@workgroup_size`, compiling the kernel if needed.
    pub fn workgroup_size(&mut self, kernel: &Kernel, kinds: &[BindKind]) -> Result<[u32; 3], String> {
        let key = PipelineKey { kernel: kernel.clone(), kinds: kinds.to_vec() };
        Ok(self.pipeline(&key)?.workgroup_size)
    }

    /// Run the kernel over `items` invocations along x — the common case, a
    /// job that is one invocation per element — computing the workgroup
    /// count from the entry point's own `@workgroup_size`. A kernel should
    /// still guard `id.x < arrayLength(...)`: the last group is padded.
    pub fn run_over(&mut self, kernel: &Kernel, bindings: &mut [Binding<'_>], items: u32) -> Result<(), String> {
        let kinds: Vec<BindKind> = bindings.iter().map(Binding::kind).collect();
        let wg = self.workgroup_size(kernel, &kinds)?;
        self.run(kernel, bindings, [workgroups(items, wg[0]), 1, 1])
    }

    /// Upload every binding, dispatch `groups` workgroups of the kernel, wait
    /// for the GPU, and read every [`Binding::Storage`] back into its slice.
    pub fn run(&mut self, kernel: &Kernel, bindings: &mut [Binding<'_>], groups: [u32; 3]) -> Result<(), String> {
        self.execute(kernel, bindings, groups, 1, None)
    }

    /// [`run_passes`](Self::run_passes) with the dispatch sized from the entry
    /// point's `@workgroup_size` over `items`, like [`run_over`](Self::run_over).
    pub fn run_passes_over(
        &mut self,
        kernel: &Kernel,
        bindings: &mut [Binding<'_>],
        items: u32,
        passes: u32,
        ping_pong: Option<(usize, usize)>,
    ) -> Result<(), String> {
        let kinds: Vec<BindKind> = bindings.iter().map(Binding::kind).collect();
        let wg = self.workgroup_size(kernel, &kinds)?;
        self.execute(kernel, bindings, [workgroups(items, wg[0]), 1, 1], passes, ping_pong)
    }

    /// `passes` dispatches of the kernel in ONE submission — uploaded once,
    /// a memory barrier between passes, waited on once, read back once —
    /// which is what an iterative solve needs: measured on an integrated
    /// GPU, a pass submitted on its own costs about half a millisecond of
    /// round trip whatever its size, and sixteen of those lose to the CPU
    /// at every mesh size a designer works at.
    ///
    /// `ping_pong = Some((a, b))` makes passes alternate the roles of two
    /// bindings: `a` must be a [`Binding::Input`] (the first pass reads it)
    /// and `b` a [`Binding::Storage`] of the same length (the first pass
    /// writes it); the second pass reads `b` and writes `a`'s buffer, and so
    /// on. Whichever buffer the LAST pass wrote is read back into `b`'s
    /// slice, so the caller always finds the result where it bound the
    /// output. A Jacobi solve is exactly this shape.
    pub fn run_passes(
        &mut self,
        kernel: &Kernel,
        bindings: &mut [Binding<'_>],
        groups: [u32; 3],
        passes: u32,
        ping_pong: Option<(usize, usize)>,
    ) -> Result<(), String> {
        self.execute(kernel, bindings, groups, passes, ping_pong)
    }

    fn execute(
        &mut self,
        kernel: &Kernel,
        bindings: &mut [Binding<'_>],
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
        let kinds: Vec<BindKind> = bindings.iter().map(Binding::kind).collect();
        let key = PipelineKey { kernel: kernel.clone(), kinds };
        let (pipeline, layout, set_layout) = {
            let p = self.pipeline(&key)?;
            (p.pipeline, p.layout, p.set_layout)
        };

        // Buffers: one per binding index, reused when big enough. Uploads are
        // memcpys through the persistent mapping.
        let device = self.core.device.clone();
        let mut sizes = Vec::with_capacity(bindings.len());
        for (i, b) in bindings.iter().enumerate() {
            let bytes = b.bytes();
            let padded = bytes.len().max(BUFFER_ALIGN).div_ceil(BUFFER_ALIGN) * BUFFER_ALIGN;
            if b.kind() == BindKind::Uniform {
                let cap = unsafe {
                    self.core.instance.get_physical_device_properties(self.core.physical_device).limits.max_uniform_buffer_range
                } as usize;
                if padded > cap {
                    return Err(format!("binding {i}: a uniform block of {} bytes exceeds the device's {cap}", bytes.len()));
                }
            }
            if i >= self.slots.len() {
                self.slots.push(AllocatedBuffer::null());
            }
            if (self.slots[i].size as usize) < padded {
                let allocator = self.core.allocator.as_mut().unwrap();
                destroy_cpu_buffer(&device, allocator, &mut self.slots[i]);
                self.slots[i] = create_cpu_buffer(
                    &device,
                    allocator,
                    padded as vk::DeviceSize,
                    vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::UNIFORM_BUFFER,
                    "compute-binding",
                );
            }
            let mapped = self.slots[i]
                .allocation
                .as_mut()
                .and_then(|a| a.mapped_slice_mut())
                .ok_or_else(|| format!("binding {i}: buffer memory is not host-visible"))?;
            mapped[..bytes.len()].copy_from_slice(bytes);
            // The padding is defined too, so an `arrayLength` that counts it
            // reads zeros rather than whatever the last job left there.
            for b in &mut mapped[bytes.len()..padded] {
                *b = 0;
            }
            sizes.push(padded);
        }

        unsafe {
            // One descriptor set per run, from a pool reset each time.
            device
                .reset_descriptor_pool(self.descriptor_pool, vk::DescriptorPoolResetFlags::empty())
                .map_err(|e| format!("descriptor pool reset: {e}"))?;
            // One descriptor set, or two with the ping-pong pair swapped in
            // the second, so alternate passes bind the buffers the other
            // way round without a write between dispatches.
            let set_count = if ping_pong.is_some() { 2 } else { 1 };
            let set_layouts = vec![set_layout; set_count];
            let sets = device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(self.descriptor_pool)
                        .set_layouts(&set_layouts),
                )
                .map_err(|e| format!("descriptor set: {e}"))?;
            let slot_for = |binding: usize, swapped: bool| -> usize {
                match ping_pong {
                    Some((a, b)) if swapped && binding == a => b,
                    Some((a, b)) if swapped && binding == b => a,
                    _ => binding,
                }
            };
            let mut infos: Vec<[vk::DescriptorBufferInfo; 1]> = Vec::with_capacity(set_count * bindings.len());
            for (si, _) in sets.iter().enumerate() {
                for i in 0..bindings.len() {
                    let slot = slot_for(i, si == 1);
                    infos.push([vk::DescriptorBufferInfo::default()
                        .buffer(self.slots[slot].buffer)
                        .offset(0)
                        .range(sizes[slot] as vk::DeviceSize)]);
                }
            }
            let mut writes: Vec<vk::WriteDescriptorSet> = Vec::with_capacity(infos.len());
            for (si, set) in sets.iter().enumerate() {
                for (i, b) in bindings.iter().enumerate() {
                    writes.push(
                        vk::WriteDescriptorSet::default()
                            .dst_set(*set)
                            .dst_binding(i as u32)
                            .descriptor_type(match b.kind() {
                                BindKind::Storage => vk::DescriptorType::STORAGE_BUFFER,
                                BindKind::Uniform => vk::DescriptorType::UNIFORM_BUFFER,
                            })
                            .buffer_info(&infos[si * bindings.len() + i]),
                    );
                }
            }
            device.update_descriptor_sets(&writes, &[]);

            device
                .begin_command_buffer(
                    self.cmd,
                    &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .map_err(|e| format!("begin: {e}"))?;
            device.cmd_bind_pipeline(self.cmd, vk::PipelineBindPoint::COMPUTE, pipeline);
            for pass in 0..passes {
                let set = sets[if ping_pong.is_some() && pass % 2 == 1 { 1 } else { 0 }];
                device.cmd_bind_descriptor_sets(self.cmd, vk::PipelineBindPoint::COMPUTE, layout, 0, &[set], &[]);
                device.cmd_dispatch(self.cmd, groups[0], groups[1], groups[2]);
                if pass + 1 < passes {
                    // The next pass reads what this one wrote.
                    device.cmd_pipeline_barrier(
                        self.cmd,
                        vk::PipelineStageFlags::COMPUTE_SHADER,
                        vk::PipelineStageFlags::COMPUTE_SHADER,
                        vk::DependencyFlags::empty(),
                        &[vk::MemoryBarrier::default()
                            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                            .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)],
                        &[],
                        &[],
                    );
                }
            }
            // Shader writes become host-visible before the fence is signalled.
            device.cmd_pipeline_barrier(
                self.cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                    .dst_access_mask(vk::AccessFlags::HOST_READ)],
                &[],
                &[],
            );
            device.end_command_buffer(self.cmd).map_err(|e| format!("end: {e}"))?;

            let cmds = [self.cmd];
            device
                .queue_submit(self.core.queue, &[vk::SubmitInfo::default().command_buffers(&cmds)], self.fence)
                .map_err(|e| format!("submit: {e}"))?;
            device
                .wait_for_fences(&[self.fence], true, u64::MAX)
                .map_err(|e| format!("fence wait: {e}"))?;
            device.reset_fences(&[self.fence]).map_err(|e| format!("fence reset: {e}"))?;
        }

        // Read back the read-write bindings — the ping-pong output from
        // whichever buffer the last pass wrote.
        let last_written = |i: usize| -> usize {
            match ping_pong {
                Some((a, b)) if i == b && passes % 2 == 0 => a,
                _ => i,
            }
        };
        for (i, b) in bindings.iter_mut().enumerate() {
            if let Binding::Storage(out) = b {
                let mapped = self.slots[last_written(i)]
                    .allocation
                    .as_ref()
                    .and_then(|a| a.mapped_slice())
                    .ok_or_else(|| format!("binding {i}: buffer memory is not host-visible"))?;
                out.copy_from_slice(&mapped[..out.len()]);
            }
        }
        Ok(())
    }

    fn pipeline(&mut self, key: &PipelineKey) -> Result<&Pipeline, String> {
        if !self.pipelines.contains_key(key) {
            let built = self.build_pipeline(key)?;
            self.pipelines.insert(key.clone(), built);
        }
        Ok(&self.pipelines[key])
    }

    fn build_pipeline(&self, key: &PipelineKey) -> Result<Pipeline, String> {
        let (spirv, workgroup_size) = compile_kernel(&key.kernel)?;
        let device = &self.core.device;
        unsafe {
            let bindings: Vec<vk::DescriptorSetLayoutBinding> = key
                .kinds
                .iter()
                .enumerate()
                .map(|(i, k)| {
                    vk::DescriptorSetLayoutBinding::default()
                        .binding(i as u32)
                        .descriptor_type(match k {
                            BindKind::Storage => vk::DescriptorType::STORAGE_BUFFER,
                            BindKind::Uniform => vk::DescriptorType::UNIFORM_BUFFER,
                        })
                        .descriptor_count(1)
                        .stage_flags(vk::ShaderStageFlags::COMPUTE)
                })
                .collect();
            let set_layout = device
                .create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings), None)
                .map_err(|e| format!("descriptor set layout: {e}"))?;
            let set_layouts = [set_layout];
            let layout = match device.create_pipeline_layout(&vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts), None) {
                Ok(l) => l,
                Err(e) => {
                    device.destroy_descriptor_set_layout(set_layout, None);
                    return Err(format!("pipeline layout: {e}"));
                }
            };
            let module = match device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&spirv), None) {
                Ok(m) => m,
                Err(e) => {
                    device.destroy_pipeline_layout(layout, None);
                    device.destroy_descriptor_set_layout(set_layout, None);
                    return Err(format!("shader module: {e}"));
                }
            };
            let entry = CString::new(key.kernel.entry.as_str()).map_err(|e| format!("entry point name: {e}"))?;
            let pipeline = device.create_compute_pipelines(
                vk::PipelineCache::null(),
                &[vk::ComputePipelineCreateInfo::default()
                    .stage(
                        vk::PipelineShaderStageCreateInfo::default()
                            .stage(vk::ShaderStageFlags::COMPUTE)
                            .module(module)
                            .name(&entry),
                    )
                    .layout(layout)],
                None,
            );
            let pipeline = match pipeline {
                Ok(p) => p[0],
                Err((_, e)) => {
                    device.destroy_shader_module(module, None);
                    device.destroy_pipeline_layout(layout, None);
                    device.destroy_descriptor_set_layout(set_layout, None);
                    return Err(format!("compute pipeline: {e}"));
                }
            };
            Ok(Pipeline { set_layout, layout, module, pipeline, workgroup_size })
        }
    }
}

impl Drop for ComputeDevice {
    fn drop(&mut self) {
        let device = self.core.device.clone();
        unsafe {
            let _ = device.device_wait_idle();
            for (_, p) in self.pipelines.drain() {
                device.destroy_pipeline(p.pipeline, None);
                device.destroy_shader_module(p.module, None);
                device.destroy_pipeline_layout(p.layout, None);
                device.destroy_descriptor_set_layout(p.set_layout, None);
            }
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            device.destroy_fence(self.fence, None);
            // The command buffer dies with the pool in VkCore's Drop.
        }
        let allocator = self.core.allocator.as_mut().unwrap();
        for slot in &mut self.slots {
            destroy_cpu_buffer(&device, allocator, slot);
        }
    }
}

/// WGSL to SPIR-V with every failure reported, plus the entry point's
/// workgroup size. `compile_wgsl` in the renderer panics on a bad shader,
/// which is right for the toolkit's own; a kernel here may be a user's.
fn compile_kernel(kernel: &Kernel) -> Result<(Vec<u32>, [u32; 3]), String> {
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
    let options = naga::back::spv::Options {
        lang_version: (1, 0),
        flags: naga::back::spv::WriterFlags::LABEL_VARYINGS,
        ..Default::default()
    };
    let spirv = naga::back::spv::write_vec(&module, &info, &options, None).map_err(|e| format!("SPIR-V: {e}"))?;
    Ok((spirv, workgroup_size))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A device, or None with a note: these tests run on whatever Vulkan
    /// the machine has (lavapipe counts) and skip where there is none.
    fn device() -> Option<ComputeDevice> {
        match ComputeDevice::new() {
            Ok(d) => Some(d),
            Err(e) => {
                println!("skipping compute test: {e}");
                None
            }
        }
    }

    const DOUBLE: &str = r#"
@group(0) @binding(0) var<storage, read_write> data: array<f32>;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i < arrayLength(&data)) {
        data[i] = data[i] * 2.0;
    }
}"#;

    #[test]
    fn a_storage_binding_round_trips_through_the_kernel() {
        let Some(mut dev) = device() else { return };
        println!("compute on {}", dev.device_name());
        // 1001 floats: not a multiple of the 16-byte padding, so the last
        // element sits beside padding and must still come back doubled.
        let mut data: Vec<f32> = (0..1001).map(|i| i as f32 * 0.5).collect();
        let kernel = Kernel::new(DOUBLE, "main");
        dev.run_over(&kernel, &mut [Binding::rw(&mut data)], 1001).unwrap();
        for (i, v) in data.iter().enumerate() {
            assert_eq!(*v, i as f32, "element {i}");
        }
        // Again, on the same device: the pipeline and the buffer are reused.
        dev.run_over(&kernel, &mut [Binding::rw(&mut data)], 1001).unwrap();
        assert_eq!(data[1000], 2000.0);
        assert_eq!(dev.pipelines.len(), 1, "one pipeline for one kernel");
        assert_eq!(dev.slots.len(), 1);
        assert_eq!(dev.workgroup_size(&kernel, &[BindKind::Storage]).unwrap(), [64, 1, 1]);
    }

    #[test]
    fn inputs_and_uniforms_bind_beside_the_output() {
        let Some(mut dev) = device() else { return };
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct Params {
            scale: f32,
            offset: f32,
            _pad: [f32; 2],
        }
        const AXPY: &str = r#"
struct Params { scale: f32, offset: f32, pad: vec2<f32> }
@group(0) @binding(0) var<storage, read> a: array<vec4<f32>>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read_write> out: array<vec4<f32>>;
@compute @workgroup_size(32)
fn axpy(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i < arrayLength(&out)) {
        out[i] = a[i] * params.scale + vec4<f32>(params.offset);
    }
}"#;
        let a: Vec<[f32; 4]> = (0..300).map(|i| [i as f32; 4]).collect();
        let mut out = vec![[0.0f32; 4]; 300];
        let params = Params { scale: 3.0, offset: 1.0, _pad: [0.0; 2] };
        dev.run_over(
            &Kernel::new(AXPY, "axpy"),
            &mut [Binding::input(&a), Binding::uniform(&params), Binding::rw(&mut out)],
            300,
        )
        .unwrap();
        for (i, v) in out.iter().enumerate() {
            assert_eq!(*v, [i as f32 * 3.0 + 1.0; 4], "element {i}");
        }
        assert!(a.iter().enumerate().all(|(i, v)| *v == [i as f32; 4]), "an input is never written back");
    }

    #[test]
    fn a_bad_kernel_is_an_error_not_a_panic() {
        let Some(mut dev) = device() else { return };
        let mut data = vec![1.0f32; 4];
        let err = dev.run_over(&Kernel::new("fn main( {", "main"), &mut [Binding::rw(&mut data)], 4).unwrap_err();
        assert!(err.starts_with("WGSL parse error"), "{err}");
        let err = dev.run_over(&Kernel::new(DOUBLE, "nope"), &mut [Binding::rw(&mut data)], 4).unwrap_err();
        assert!(err.contains("`nope`") && err.contains("main"), "names the missing entry and the offer: {err}");
        let typed = "@group(0) @binding(0) var<storage, read_write> d: array<f32>;\n@compute @workgroup_size(1) fn main() { d[0] = 1u; }";
        let err = dev.run_over(&Kernel::new(typed, "main"), &mut [Binding::rw(&mut data)], 1).unwrap_err();
        // naga's front end types as it parses, so a type error is a parse
        // error; what matters is that it is a WGSL diagnostic, not a panic.
        assert!(err.starts_with("WGSL") && err.contains("u32"), "{err}");
        assert_eq!(data, vec![1.0; 4], "nothing ran");
        // The device is still good after every failure.
        dev.run_over(&Kernel::new(DOUBLE, "main"), &mut [Binding::rw(&mut data)], 4).unwrap();
        assert_eq!(data, vec![2.0; 4]);
    }

    /// Passes chain inside one submission, and a ping-pong pair alternates
    /// so the result lands in the output slice whether the count is odd or
    /// even.
    #[test]
    fn passes_chain_and_ping_pong_lands_in_the_output() {
        let Some(mut dev) = device() else { return };
        // In place: three doublings are one octupling.
        let mut data: Vec<f32> = (0..500).map(|i| i as f32).collect();
        dev.run_passes_over(&Kernel::new(DOUBLE, "main"), &mut [Binding::rw(&mut data)], 500, 3, None).unwrap();
        assert!(data.iter().enumerate().all(|(i, v)| *v == i as f32 * 8.0));

        const COPY_DOUBLE: &str = r#"
@group(0) @binding(0) var<storage, read> src: array<f32>;
@group(0) @binding(1) var<storage, read_write> dst: array<f32>;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i < arrayLength(&dst)) { dst[i] = src[i] * 2.0; }
}"#;
        let src: Vec<f32> = (0..500).map(|i| i as f32).collect();
        let kernel = Kernel::new(COPY_DOUBLE, "main");
        for passes in [1u32, 2, 3, 4] {
            let mut dst = vec![0.0f32; 500];
            dev.run_passes_over(&kernel, &mut [Binding::input(&src), Binding::rw(&mut dst)], 500, passes, Some((0, 1))).unwrap();
            let factor = 2f32.powi(passes as i32);
            assert!(dst.iter().enumerate().all(|(i, v)| *v == i as f32 * factor), "{passes} passes give x{factor}");
        }
        assert!(src.iter().enumerate().all(|(i, v)| *v == i as f32), "the input slice is never written");

        let mut dst = vec![0.0f32; 500];
        let err = dev.run_passes_over(&kernel, &mut [Binding::input(&src), Binding::rw(&mut dst)], 500, 2, Some((1, 0))).unwrap_err();
        assert!(err.contains("must be a read-only Input"), "{err}");
        let err = dev.run_passes_over(&kernel, &mut [Binding::input(&src), Binding::rw(&mut dst)], 500, 0, None).unwrap_err();
        assert!(err.contains("at least one pass"), "{err}");
    }

    #[test]
    fn workgroup_arithmetic() {
        assert_eq!(workgroups(0, 64), 1, "a dispatch of zero groups is invalid");
        assert_eq!(workgroups(1, 64), 1);
        assert_eq!(workgroups(64, 64), 1);
        assert_eq!(workgroups(65, 64), 2);
        assert_eq!(workgroups(1000, 0), 1000, "a zero group size is treated as one");
    }
}
