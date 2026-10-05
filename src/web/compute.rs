//! `crate::compute` jobs on WebGPU: the browser's `vk::ComputeDevice`.
//!
//! The same kernels, bindings and rules (`crate::compute`), answered the same
//! way — a ping-pong job's result lands in its output binding, every
//! read-write binding is read back into its slice — with one difference
//! the platform makes: reading a buffer back is a promise, so [`run`] and its
//! siblings are `async`. A job is still one submission, its passes in one
//! compute pass; WebGPU orders the dispatches of a pass by itself.
//!
//! A kernel is parsed and validated by naga before WebGPU sees it, so a
//! user's bad WGSL is an `Err` with naga's diagnostic, as natively; what
//! WebGPU still rejects (a limit, a layout) is caught in a validation error
//! scope and returned too. The device asks for the adapter's own limits on
//! storage buffers and workgroups: WebGPU's defaults allow eight storage
//! buffers a stage, where a job may carry sixteen. What the adapter offers
//! is the ceiling — SwiftShader's is ten — and a job past it is an `Err`
//! naming the limit. (cce-designer's kernels bind at most seven.)
//!
//! [`run`]: ComputeDevice::run

use std::collections::HashMap;

use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    gpu_buffer_usage as buffer_usage, gpu_map_mode as map_mode, gpu_shader_stage as shader_stage, GpuBindGroupDescriptor,
    GpuBindGroupEntry, GpuBindGroupLayout, GpuBindGroupLayoutDescriptor, GpuBindGroupLayoutEntry, GpuBuffer,
    GpuBufferBinding, GpuBufferBindingLayout, GpuBufferBindingType, GpuBufferDescriptor, GpuComputePassDescriptor,
    GpuComputePipeline, GpuComputePipelineDescriptor, GpuDevice, GpuErrorFilter, GpuPipelineLayoutDescriptor,
    GpuProgrammableStage, GpuQueue, GpuShaderModuleDescriptor,
};

use crate::compute::{
    check_job, padded_len, parse_kernel, result_slot, slot_for, workgroups, BindKind, Binding, Kernel,
};

/// The limits a compute device asks the adapter for in full.
const LIMITS: &[&str] = &[
    "maxStorageBuffersPerShaderStage",
    "maxUniformBuffersPerShaderStage",
    "maxStorageBufferBindingSize",
    "maxUniformBufferBindingSize",
    "maxBufferSize",
    "maxComputeWorkgroupStorageSize",
    "maxComputeInvocationsPerWorkgroup",
    "maxComputeWorkgroupSizeX",
    "maxComputeWorkgroupSizeY",
    "maxComputeWorkgroupSizeZ",
    "maxComputeWorkgroupsPerDimension",
];

#[derive(Clone, PartialEq, Eq, Hash)]
struct PipelineKey {
    kernel: Kernel,
    kinds: Vec<BindKind>,
}

struct Pipeline {
    pipeline: GpuComputePipeline,
    layout: GpuBindGroupLayout,
    workgroup_size: [u32; 3],
}

/// A buffer and its size in bytes.
struct Slot {
    buffer: GpuBuffer,
    size: usize,
}

/// A WebGPU device that runs compute jobs. See the module docs.
pub struct ComputeDevice {
    _gpu: web_sys::Gpu,
    adapter: web_sys::GpuAdapter,
    device: GpuDevice,
    queue: GpuQueue,
    pipelines: HashMap<PipelineKey, Pipeline>,
    /// One buffer per binding index, grown when a job needs more room.
    slots: Vec<Option<Slot>>,
    /// The mappable copies read-write bindings are read back through.
    readback: Vec<Option<Slot>>,
}

fn js_err(e: JsValue) -> String {
    e.as_string()
        .or_else(|| js_sys::Reflect::get(&e, &"message".into()).ok().and_then(|m| m.as_string()))
        .unwrap_or_else(|| format!("{e:?}"))
}

impl ComputeDevice {
    /// A device on the browser's WebGPU adapter. `Err` when the browser
    /// offers none.
    pub async fn new() -> Result<Self, String> {
        let (gpu, adapter, device) =
            super::request_device(LIMITS).await.map_err(|e| format!("no WebGPU compute device: {}", js_err(e)))?;
        let queue = device.queue();
        Ok(Self { _gpu: gpu, adapter, device, queue, pipelines: HashMap::new(), slots: Vec::new(), readback: Vec::new() })
    }

    /// The adapter as the browser describes it, for a log line or a status
    /// readout. A browser may say little: it guards what fingerprints a machine.
    pub fn device_name(&self) -> String {
        let info = self.adapter.info();
        let parts: Vec<String> =
            [info.vendor(), info.architecture(), info.device(), info.description()].into_iter().filter(|s| !s.is_empty()).collect();
        if parts.is_empty() { "WebGPU".into() } else { format!("WebGPU {}", parts.join(" ")) }
    }

    /// The entry point's `@workgroup_size`, compiling the kernel if needed.
    pub async fn workgroup_size(&mut self, kernel: &Kernel, kinds: &[BindKind]) -> Result<[u32; 3], String> {
        let key = PipelineKey { kernel: kernel.clone(), kinds: kinds.to_vec() };
        Ok(self.pipeline(&key).await?.workgroup_size)
    }

    /// Run the kernel over `items` invocations along x, the workgroup count
    /// from the entry point's own `@workgroup_size` (see `vk::ComputeDevice::run_over`).
    pub async fn run_over(&mut self, kernel: &Kernel, bindings: &mut [Binding<'_>], items: u32) -> Result<(), String> {
        let kinds: Vec<BindKind> = bindings.iter().map(Binding::kind).collect();
        let wg = self.workgroup_size(kernel, &kinds).await?;
        self.execute(kernel, bindings, [workgroups(items, wg[0]), 1, 1], 1, None).await
    }

    /// Upload every binding, dispatch `groups` workgroups of the kernel, and
    /// read every [`Binding::Storage`] back into its slice once the GPU is done.
    pub async fn run(&mut self, kernel: &Kernel, bindings: &mut [Binding<'_>], groups: [u32; 3]) -> Result<(), String> {
        self.execute(kernel, bindings, groups, 1, None).await
    }

    /// [`run_passes`](Self::run_passes) sized over `items`, like [`run_over`](Self::run_over).
    pub async fn run_passes_over(
        &mut self,
        kernel: &Kernel,
        bindings: &mut [Binding<'_>],
        items: u32,
        passes: u32,
        ping_pong: Option<(usize, usize)>,
    ) -> Result<(), String> {
        let kinds: Vec<BindKind> = bindings.iter().map(Binding::kind).collect();
        let wg = self.workgroup_size(kernel, &kinds).await?;
        self.execute(kernel, bindings, [workgroups(items, wg[0]), 1, 1], passes, ping_pong).await
    }

    /// `passes` dispatches of the kernel in one submission, with an optional
    /// ping-pong pair — the semantics of `vk::ComputeDevice::run_passes`.
    pub async fn run_passes(
        &mut self,
        kernel: &Kernel,
        bindings: &mut [Binding<'_>],
        groups: [u32; 3],
        passes: u32,
        ping_pong: Option<(usize, usize)>,
    ) -> Result<(), String> {
        self.execute(kernel, bindings, groups, passes, ping_pong).await
    }

    async fn execute(
        &mut self,
        kernel: &Kernel,
        bindings: &mut [Binding<'_>],
        groups: [u32; 3],
        passes: u32,
        ping_pong: Option<(usize, usize)>,
    ) -> Result<(), String> {
        check_job(bindings, groups, passes, ping_pong)?;
        let kinds: Vec<BindKind> = bindings.iter().map(Binding::kind).collect();
        let key = PipelineKey { kernel: kernel.clone(), kinds };
        let (pipeline, layout) = {
            let p = self.pipeline(&key).await?;
            (p.pipeline.clone(), p.layout.clone())
        };

        // Buffers: one per binding index, reused when big enough. The
        // padding is uploaded too, as zeros, so an `arrayLength` that counts
        // it reads what it would natively.
        let uniform_cap = self.device.limits().max_uniform_buffer_binding_size() as usize;
        let mut sizes = Vec::with_capacity(bindings.len());
        for (i, b) in bindings.iter().enumerate() {
            let bytes = b.bytes();
            let padded = padded_len(bytes.len());
            if b.kind() == BindKind::Uniform && padded > uniform_cap {
                return Err(format!("binding {i}: a uniform block of {} bytes exceeds the device's {uniform_cap}", bytes.len()));
            }
            let usage = buffer_usage::STORAGE | buffer_usage::UNIFORM | buffer_usage::COPY_SRC | buffer_usage::COPY_DST;
            ensure(&self.device, &mut self.slots, i, padded, usage, "compute-binding")?;
            let mut upload = bytes.to_vec();
            upload.resize(padded, 0);
            let slot = self.slots[i].as_ref().unwrap();
            self.queue.write_buffer_with_u32_and_u8_slice(&slot.buffer, 0, &upload).map_err(js_err)?;
            sizes.push(padded);
        }

        self.device.push_error_scope(GpuErrorFilter::Validation);
        // One bind group, or two with the ping-pong pair swapped in the
        // second, so alternate passes bind the buffers the other way round.
        let group_count = if ping_pong.is_some() { 2 } else { 1 };
        let mut groups_bound = Vec::with_capacity(group_count);
        for g in 0..group_count {
            let entries: Vec<GpuBindGroupEntry> = (0..bindings.len())
                .map(|i| {
                    let slot = slot_for(i, g == 1, ping_pong);
                    let binding = GpuBufferBinding::new(&self.slots[slot].as_ref().unwrap().buffer);
                    binding.set_size(sizes[slot] as u32);
                    GpuBindGroupEntry::new_with_gpu_buffer_binding(i as u32, &binding)
                })
                .collect();
            groups_bound.push(self.device.create_bind_group(&GpuBindGroupDescriptor::new(&entries, &layout)));
        }
        let encoder = self.device.create_command_encoder();
        let pass = encoder.begin_compute_pass_with_descriptor(&GpuComputePassDescriptor::new());
        pass.set_pipeline(&pipeline);
        for p in 0..passes {
            let g = if ping_pong.is_some() && p % 2 == 1 { 1 } else { 0 };
            pass.set_bind_group(0, Some(&groups_bound[g]));
            pass.dispatch_workgroups_with_workgroup_count_y_and_workgroup_count_z(groups[0], groups[1], groups[2]);
        }
        pass.end();

        // Copy every read-write binding's result into a mappable buffer.
        let mut reads = Vec::new();
        for (i, b) in bindings.iter().enumerate() {
            if let Binding::Storage(out) = b {
                let from = result_slot(i, passes, ping_pong);
                let len = padded_len(out.len());
                ensure(&self.device, &mut self.readback, i, len, buffer_usage::MAP_READ | buffer_usage::COPY_DST, "compute-readback")?;
                encoder
                    .copy_buffer_to_buffer_with_u32_and_u32_and_u32(
                        &self.slots[from].as_ref().unwrap().buffer,
                        0,
                        &self.readback[i].as_ref().unwrap().buffer,
                        0,
                        len as u32,
                    )
                    .map_err(js_err)?;
                reads.push(i);
            }
        }
        self.queue.submit(&[encoder.finish()]);
        if let Some(err) = wasm_bindgen_futures::JsFuture::from(self.device.pop_error_scope()).await.map_err(js_err)?.dyn_ref::<web_sys::GpuError>()
        {
            return Err(format!("WebGPU rejected the job: {}", err.message()));
        }

        for i in reads {
            let Binding::Storage(out) = &mut bindings[i] else { unreachable!() };
            let buffer = &self.readback[i].as_ref().unwrap().buffer;
            wasm_bindgen_futures::JsFuture::from(buffer.map_async_with_u32_and_u32(map_mode::READ, 0, padded_len(out.len()) as u32))
                .await
                .map_err(|e| format!("binding {i}: readback: {}", js_err(e)))?;
            let mapped = js_sys::Uint8Array::new(&buffer.get_mapped_range().map_err(js_err)?.into());
            mapped.subarray(0, out.len() as u32).copy_to(out);
            buffer.unmap();
        }
        Ok(())
    }

    async fn pipeline(&mut self, key: &PipelineKey) -> Result<&Pipeline, String> {
        if !self.pipelines.contains_key(key) {
            let built = self.build_pipeline(key).await?;
            self.pipelines.insert(key.clone(), built);
        }
        Ok(&self.pipelines[key])
    }

    async fn build_pipeline(&self, key: &PipelineKey) -> Result<Pipeline, String> {
        let parsed = parse_kernel(&key.kernel)?;
        // Read-only storage is its own binding type here (WebGPU tells it
        // from read-write, which Vulkan does not): the module says which.
        let entries: Vec<GpuBindGroupLayoutEntry> = key
            .kinds
            .iter()
            .enumerate()
            .map(|(i, k)| {
                let ty = match k {
                    BindKind::Uniform => GpuBufferBindingType::Uniform,
                    BindKind::Storage if parsed.read_only_storage(i as u32) => GpuBufferBindingType::ReadOnlyStorage,
                    BindKind::Storage => GpuBufferBindingType::Storage,
                };
                let buffer = GpuBufferBindingLayout::new();
                buffer.set_type(ty);
                let entry = GpuBindGroupLayoutEntry::new(i as u32, shader_stage::COMPUTE);
                entry.set_buffer(&buffer);
                entry
            })
            .collect();
        self.device.push_error_scope(GpuErrorFilter::Validation);
        let layout = self.device.create_bind_group_layout(&GpuBindGroupLayoutDescriptor::new(&entries)).map_err(js_err)?;
        let pipeline_layout = self.device.create_pipeline_layout(&GpuPipelineLayoutDescriptor::new(&[js_sys::JsOption::wrap(layout.clone())]));
        let module = self.device.create_shader_module(&GpuShaderModuleDescriptor::new(&key.kernel.source));
        let stage = GpuProgrammableStage::new(&module);
        stage.set_entry_point(&key.kernel.entry);
        let pipeline = self.device.create_compute_pipeline(&GpuComputePipelineDescriptor::new(&pipeline_layout, &stage));
        if let Some(err) = wasm_bindgen_futures::JsFuture::from(self.device.pop_error_scope()).await.map_err(js_err)?.dyn_ref::<web_sys::GpuError>()
        {
            return Err(format!("compute pipeline: {}", err.message()));
        }
        Ok(Pipeline { pipeline, layout, workgroup_size: parsed.workgroup_size })
    }
}

/// `slots[i]` holds a buffer of at least `size` bytes with `usage`.
fn ensure(device: &GpuDevice, slots: &mut Vec<Option<Slot>>, i: usize, size: usize, usage: u32, label: &str) -> Result<(), String> {
    if slots.len() <= i {
        slots.resize_with(i + 1, || None);
    }
    if slots[i].as_ref().is_some_and(|s| s.size >= size) {
        return Ok(());
    }
    if let Some(old) = slots[i].take() {
        old.buffer.destroy();
    }
    let desc = GpuBufferDescriptor::new(size as u32, usage);
    desc.set_label(label);
    slots[i] = Some(Slot { buffer: device.create_buffer(&desc).map_err(js_err)?, size });
    Ok(())
}

impl Drop for ComputeDevice {
    fn drop(&mut self) {
        for s in self.slots.iter().chain(self.readback.iter()).flatten() {
            s.buffer.destroy();
        }
    }
}
