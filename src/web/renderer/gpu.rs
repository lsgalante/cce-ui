//! WebGPU helpers the renderer (and the 3D and tracer passes beside it) build with: shader modules,
//! alpha blending, bind-group layout entries, samplers, textures and their views, texture writes,
//! and uniform bindings.

use super::*;

pub(in crate::web) fn shader_module(device: &GpuDevice, code: &str, label: &str) -> web_sys::GpuShaderModule {
    let desc = GpuShaderModuleDescriptor::new(code);
    desc.set_label(label);
    device.create_shader_module(&desc)
}

/// `wgpu::BlendState::ALPHA_BLENDING`, as both Vulkan pipelines blend.
pub(in crate::web) fn alpha_blending() -> GpuBlendState {
    let color = GpuBlendComponent::new();
    color.set_src_factor(GpuBlendFactor::SrcAlpha);
    color.set_dst_factor(GpuBlendFactor::OneMinusSrcAlpha);
    color.set_operation(GpuBlendOperation::Add);
    let alpha = GpuBlendComponent::new();
    alpha.set_src_factor(GpuBlendFactor::One);
    alpha.set_dst_factor(GpuBlendFactor::OneMinusSrcAlpha);
    alpha.set_operation(GpuBlendOperation::Add);
    GpuBlendState::new(&alpha, &color)
}

pub(super) fn uniform_entry(binding: u32, min_size: u32, dynamic: bool) -> GpuBindGroupLayoutEntry {
    let entry = GpuBindGroupLayoutEntry::new(binding, shader_stage::FRAGMENT);
    let layout = GpuBufferBindingLayout::new();
    layout.set_type(GpuBufferBindingType::Uniform);
    layout.set_min_binding_size(min_size);
    layout.set_has_dynamic_offset(dynamic);
    entry.set_buffer(&layout);
    entry
}

pub(super) fn texture_entry(binding: u32) -> GpuBindGroupLayoutEntry {
    let entry = GpuBindGroupLayoutEntry::new(binding, shader_stage::FRAGMENT);
    let layout = GpuTextureBindingLayout::new();
    layout.set_sample_type(GpuTextureSampleType::Float);
    layout.set_view_dimension(GpuTextureViewDimension::N2d);
    entry.set_texture(&layout);
    entry
}

pub(super) fn sampler_entry(binding: u32) -> GpuBindGroupLayoutEntry {
    let entry = GpuBindGroupLayoutEntry::new(binding, shader_stage::FRAGMENT);
    let layout = GpuSamplerBindingLayout::new();
    layout.set_type(GpuSamplerBindingType::Filtering);
    entry.set_sampler(&layout);
    entry
}

pub(super) fn sampler(device: &GpuDevice, filter: GpuFilterMode, mipmap: GpuMipmapFilterMode) -> GpuSampler {
    let desc = GpuSamplerDescriptor::new();
    desc.set_address_mode_u(GpuAddressMode::ClampToEdge);
    desc.set_address_mode_v(GpuAddressMode::ClampToEdge);
    desc.set_address_mode_w(GpuAddressMode::ClampToEdge);
    desc.set_mag_filter(filter);
    desc.set_min_filter(filter);
    desc.set_mipmap_filter(mipmap);
    device.create_sampler_with_descriptor(&desc)
}

pub(in crate::web) fn texture(device: &GpuDevice, format: GpuTextureFormat, w: u32, h: u32, usage: u32, label: &str) -> Result<GpuTexture, JsValue> {
    let size = [js_sys::Number::from(w.max(1)), js_sys::Number::from(h.max(1))];
    let desc = GpuTextureDescriptor::new(format, &size, usage);
    desc.set_label(label);
    device.create_texture(&desc)
}

pub(in crate::web) fn whole_view(texture: &GpuTexture) -> Result<GpuTextureView, JsValue> {
    texture.create_view()
}

pub(super) fn extent(w: u32, h: u32) -> GpuExtent3dDict {
    let e = GpuExtent3dDict::new(w);
    e.set_height(h);
    e
}

/// Write `pixels` (`w` x `h`, 4 bytes a texel, rows packed) into `texture`.
pub(super) fn write_texture(queue: &GpuQueue, texture: &GpuTexture, pixels: &[u8], w: u32, h: u32) -> Result<(), JsValue> {
    let layout = GpuTexelCopyBufferLayout::new();
    layout.set_bytes_per_row(w * 4);
    layout.set_rows_per_image(h);
    queue.write_texture_with_u8_slice_and_gpu_extent_3d_dict(
        &GpuTexelCopyTextureInfo::new(texture),
        pixels,
        &layout,
        &extent(w, h),
    )
}

/// [`write_texture`] for one rectangle of `texture`, at `(x, y)`: `pixels`
/// holds just that rectangle, tightly packed.
pub(super) fn write_texture_region(
    queue: &GpuQueue,
    texture: &GpuTexture,
    pixels: &[u8],
    (x, y, w, h): (u32, u32, u32, u32),
) -> Result<(), JsValue> {
    let layout = GpuTexelCopyBufferLayout::new();
    layout.set_bytes_per_row(w * 4);
    layout.set_rows_per_image(h);
    let origin = GpuOrigin3dDict::new();
    origin.set_x(x);
    origin.set_y(y);
    let target = GpuTexelCopyTextureInfo::new(texture);
    target.set_origin_gpu_origin_3d_dict(&origin);
    queue.write_texture_with_u8_slice_and_gpu_extent_3d_dict(&target, pixels, &layout, &extent(w, h))
}

/// A two-entry (texture, sampler) bind group for the glyph shader.
pub(super) fn texture_group(device: &GpuDevice, layout: &GpuBindGroupLayout, view: &GpuTextureView, sampler: &GpuSampler) -> GpuBindGroup {
    let entries = [GpuBindGroupEntry::new_with_gpu_texture_view(0, view), GpuBindGroupEntry::new(1, sampler)];
    device.create_bind_group(&GpuBindGroupDescriptor::new(&entries, layout))
}

pub(super) fn uniform_binding(buffer: &GpuBuffer, size: u32) -> GpuBufferBinding {
    let binding = GpuBufferBinding::new(buffer);
    binding.set_size(size);
    binding
}
