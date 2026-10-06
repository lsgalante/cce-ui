//! An offscreen 2D render for tests: a `DisplayList` tessellated by the
//! runner's own `tessellate_display_list`, drawn through the live UI pipeline
//! (`renderer::create_ui_pipeline`, the same push constants and `WindowInfo`)
//! into an image, and read back. What it does not do is what a plate test does
//! not need: no text, no images, no 3D backdrop (a 1x1 clear one is bound),
//! and no blur-behind — a frosted plate is refused, since it samples a
//! snapshot of the frame-so-far that only the swapchain path takes.
//!
//! It exists so the 2D shader can be TESTED rather than read: the first test
//! holds a carve grouped into its plate to the same carve drawn as an overlay,
//! pixel for pixel (see cce-ui/CLAUDE.md, "A grouped carve shades as its
//! overlay does"). `render` returns `None` where there is no Vulkan device,
//! and the tests skip with a note, as the GPU cross-checks elsewhere do.

use ash::vk;
use gpu_allocator::vulkan::{AllocationCreateDesc, AllocationScheme};
use gpu_allocator::MemoryLocation;

use super::core::VkCore;
use super::renderer::{
    batch_push_constants, clear_image_to_shader_read, create_cpu_buffer, create_ui_pipeline,
    destroy_cpu_buffer, flipped_viewport, relief_px_at, window_info_data, FRAMES_IN_FLIGHT,
    MAX_PLATE_FEATURES, PLATE_FEATURE_BYTES, WINDOW_INFO_BYTES,
};
use crate::scene::paint::DisplayList;

/// The format the renderer prefers for its swapchain, so blending happens in
/// the same (linear) space.
const FORMAT: vk::Format = vk::Format::R8G8B8A8_SRGB;

/// A rendered frame: `width * height` RGBA8 pixels (sRGB-encoded), rows top
/// to bottom.
pub(crate) struct Rendered {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Rendered {
    pub fn rgba(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [self.pixels[i], self.pixels[i + 1], self.pixels[i + 2], self.pixels[i + 3]]
    }
}

/// Render `dl` at `logical_w` x `logical_h` logical px and `scale`, over an
/// opaque black clear. `None` when no Vulkan device can be opened.
pub(crate) fn render(dl: &DisplayList, logical_w: f32, logical_h: f32, scale: f32) -> Option<Rendered> {
    let mut core = std::panic::catch_unwind(VkCore::new_headless).ok()?;
    let (verts, dl_batches, _images, features) =
        crate::backend::tessellate::tessellate_display_list(dl, logical_w, logical_h, scale);
    let batches = crate::backend::tessellate::dl_batches_2d(&dl_batches, scale);
    assert!(
        batches.iter().all(|b| !b.blur_behind),
        "plate_probe draws no blur-behind batch: give the plates an unfrosted material"
    );
    assert!(features.len() <= MAX_PLATE_FEATURES, "more carves than one frame's feature slot");
    let width = (logical_w * scale).round() as u32;
    let height = (logical_h * scale).round() as u32;
    let extent = vk::Extent2D { width, height };
    let clip_shape = crate::layout::corner_shape();

    unsafe {
        let device = core.device.clone();
        let queue = core.queue;
        let pool = core.command_pool;
        let allocator = core.allocator.as_mut().expect("allocator");

        // Target: the colour attachment, then the copy source.
        let mut image_of = |w: u32, h: u32, usage: vk::ImageUsageFlags, name: &str| {
            let image = device
                .create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(FORMAT)
                        .extent(vk::Extent3D { width: w, height: h, depth: 1 })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(usage)
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )
                .expect("plate_probe image");
            let allocation = allocator
                .allocate(&AllocationCreateDesc {
                    name,
                    requirements: device.get_image_memory_requirements(image),
                    location: MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .expect("plate_probe image memory");
            device
                .bind_image_memory(image, allocation.memory(), allocation.offset())
                .expect("plate_probe bind image");
            let view = device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(FORMAT)
                        .subresource_range(color_range()),
                    None,
                )
                .expect("plate_probe view");
            (image, allocation, view)
        };
        let (target, target_mem, target_view) = image_of(
            width,
            height,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
            "plate-probe-target",
        );
        // The backdrop binding: nothing samples it without blur-behind.
        let (backdrop, backdrop_mem, backdrop_view) = image_of(
            1,
            1,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
            "plate-probe-backdrop",
        );
        clear_image_to_shader_read(&device, queue, pool, backdrop);

        let attachments = [vk::AttachmentDescription::default()
            .format(FORMAT)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)];
        let color_refs = [vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
        let subpasses = [vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_refs)];
        let render_pass = device
            .create_render_pass(
                &vk::RenderPassCreateInfo::default().attachments(&attachments).subpasses(&subpasses),
                None,
            )
            .expect("plate_probe render pass");
        let (set_layout, pipeline_layout, shader_module, pipeline) = create_ui_pipeline(&device, render_pass);
        let framebuffer_views = [target_view];
        let framebuffer = device
            .create_framebuffer(
                &vk::FramebufferCreateInfo::default()
                    .render_pass(render_pass)
                    .attachments(&framebuffer_views)
                    .width(width)
                    .height(height)
                    .layers(1),
                None,
            )
            .expect("plate_probe framebuffer");

        // Buffers: vertices, WindowInfo, the feature UBO (slot 0 of the
        // renderer's two), the readback.
        // All the renderer's frame slots, as the shader declares them; the
        // features go in slot 0, so their offsets need no rebase.
        let feature_bytes = (FRAMES_IN_FLIGHT * MAX_PLATE_FEATURES * PLATE_FEATURE_BYTES) as vk::DeviceSize;
        let vert_bytes: &[u8] = bytemuck::cast_slice(&verts);
        let mut vbuf = create_cpu_buffer(
            &device,
            allocator,
            (vert_bytes.len() as vk::DeviceSize).max(64),
            vk::BufferUsageFlags::VERTEX_BUFFER,
            "plate-probe-verts",
        );
        let mut info = create_cpu_buffer(&device, allocator, WINDOW_INFO_BYTES, vk::BufferUsageFlags::UNIFORM_BUFFER, "plate-probe-info");
        let mut feat = create_cpu_buffer(&device, allocator, feature_bytes, vk::BufferUsageFlags::UNIFORM_BUFFER, "plate-probe-features");
        let readback_bytes = (width as vk::DeviceSize) * (height as vk::DeviceSize) * 4;
        let mut readback = create_cpu_buffer(&device, allocator, readback_bytes, vk::BufferUsageFlags::TRANSFER_DST, "plate-probe-readback");
        vbuf.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()[..vert_bytes.len()].copy_from_slice(vert_bytes);
        let info_data = window_info_data(extent, 0.0, relief_px_at(scale));
        info.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()[..WINDOW_INFO_BYTES as usize]
            .copy_from_slice(bytemuck::cast_slice(&info_data));
        let feat_bytes: &[u8] = bytemuck::cast_slice(&features);
        feat.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()[..feat_bytes.len()].copy_from_slice(feat_bytes);

        let sampler = device
            .create_sampler(&vk::SamplerCreateInfo::default(), None)
            .expect("plate_probe sampler");
        let pool_sizes = [
            vk::DescriptorPoolSize::default().ty(vk::DescriptorType::SAMPLED_IMAGE).descriptor_count(1),
            vk::DescriptorPoolSize::default().ty(vk::DescriptorType::SAMPLER).descriptor_count(1),
            vk::DescriptorPoolSize::default().ty(vk::DescriptorType::UNIFORM_BUFFER).descriptor_count(2),
        ];
        let descriptor_pool = device
            .create_descriptor_pool(&vk::DescriptorPoolCreateInfo::default().max_sets(1).pool_sizes(&pool_sizes), None)
            .expect("plate_probe descriptor pool");
        let layouts = [set_layout];
        let set = device
            .allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default().descriptor_pool(descriptor_pool).set_layouts(&layouts),
            )
            .expect("plate_probe descriptor set")[0];
        let image_infos = [vk::DescriptorImageInfo::default()
            .image_view(backdrop_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let sampler_infos = [vk::DescriptorImageInfo::default().sampler(sampler)];
        let info_infos = [vk::DescriptorBufferInfo::default().buffer(info.buffer).offset(0).range(WINDOW_INFO_BYTES)];
        let feat_infos = [vk::DescriptorBufferInfo::default().buffer(feat.buffer).offset(0).range(feature_bytes)];
        device.update_descriptor_sets(
            &[
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                    .image_info(&image_infos),
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::SAMPLER)
                    .image_info(&sampler_infos),
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&info_infos),
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(3)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&feat_infos),
            ],
            &[],
        );

        let cmd = device
            .allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
            .expect("plate_probe command buffer")[0];
        device
            .begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT))
            .unwrap();
        let clear = [vk::ClearValue { color: vk::ClearColorValue { float32: [0.0, 0.0, 0.0, 1.0] } }];
        let full = vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent };
        device.cmd_begin_render_pass(
            cmd,
            &vk::RenderPassBeginInfo::default()
                .render_pass(render_pass)
                .framebuffer(framebuffer)
                .render_area(full)
                .clear_values(&clear),
            vk::SubpassContents::INLINE,
        );
        device.cmd_set_viewport(cmd, 0, &[flipped_viewport(extent)]);
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
        device.cmd_bind_descriptor_sets(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline_layout, 0, &[set], &[]);
        device.cmd_bind_vertex_buffers(cmd, 0, &[vbuf.buffer], &[0]);
        for batch in &batches {
            if batch.start >= batch.end {
                continue;
            }
            let scissor = match batch.scissor {
                Some((x, y, w, h)) => {
                    if x >= width || y >= height {
                        continue;
                    }
                    let (w, h) = (w.min(width - x), h.min(height - y));
                    if w == 0 || h == 0 {
                        continue;
                    }
                    vk::Rect2D { offset: vk::Offset2D { x: x as i32, y: y as i32 }, extent: vk::Extent2D { width: w, height: h } }
                }
                None => full,
            };
            device.cmd_set_scissor(cmd, 0, &[scissor]);
            let pc = batch_push_constants(batch, clip_shape, 0);
            device.cmd_push_constants(cmd, pipeline_layout, vk::ShaderStageFlags::FRAGMENT, 0, bytemuck::cast_slice(&pc));
            device.cmd_draw(cmd, batch.end - batch.start, 1, batch.start, 0);
        }
        device.cmd_end_render_pass(cmd);
        device.cmd_copy_image_to_buffer(
            cmd,
            target,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            readback.buffer,
            &[vk::BufferImageCopy::default()
                .image_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .layer_count(1),
                )
                .image_extent(vk::Extent3D { width, height, depth: 1 })],
        );
        device.end_command_buffer(cmd).unwrap();
        let fence = device.create_fence(&vk::FenceCreateInfo::default(), None).expect("plate_probe fence");
        let cmds = [cmd];
        device
            .queue_submit(queue, &[vk::SubmitInfo::default().command_buffers(&cmds)], fence)
            .expect("plate_probe submit");
        device.wait_for_fences(&[fence], true, u64::MAX).expect("plate_probe wait");
        let pixels = readback.allocation.as_ref().unwrap().mapped_slice().unwrap()[..readback_bytes as usize].to_vec();

        device.destroy_fence(fence, None);
        device.free_command_buffers(pool, &cmds);
        device.destroy_descriptor_pool(descriptor_pool, None);
        device.destroy_sampler(sampler, None);
        for b in [&mut vbuf, &mut info, &mut feat, &mut readback] {
            destroy_cpu_buffer(&device, allocator, b);
        }
        device.destroy_framebuffer(framebuffer, None);
        device.destroy_pipeline(pipeline, None);
        device.destroy_pipeline_layout(pipeline_layout, None);
        device.destroy_shader_module(shader_module, None);
        device.destroy_descriptor_set_layout(set_layout, None);
        device.destroy_render_pass(render_pass, None);
        for (image, mem, view) in [(target, target_mem, target_view), (backdrop, backdrop_mem, backdrop_view)] {
            device.destroy_image_view(view, None);
            device.destroy_image(image, None);
            allocator.free(mem).expect("plate_probe free");
        }
        Some(Rendered { width, height, pixels })
    }
}

fn color_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::layout::Rect;
    use crate::scene::paint::PaintCtx;
    use crate::scene::Material;

    /// Two identical plates of recesses (at the parameter pane's sizes, a
    /// boss among them): the left one's carves group into it as CSG
    /// features, the right one's are each their own overlay, because a
    /// transparent quad painted first closes the plate's grouping window.
    fn two_plates(w: f32, h: f32) -> DisplayList {
        let mut pc = PaintCtx::new();
        let half = (w - 30.0) * 0.5;
        let bevel = crate::layout::bevel_width();
        for (i, x) in [10.0, 20.0 + half].into_iter().enumerate() {
            let plate = Rect { x, y: 10.0, width: half, height: h - 20.0 };
            pc.plate(plate, (16.0, 16.0, 16.0, 16.0), &Material::opaque([0.13, 0.13, 0.15, 1.0]), bevel);
            if i == 1 {
                pc.quad(Rect { x: x + 1.0, y: 11.0, width: 1.0, height: 1.0 }, [0.0; 4]);
            }
            let mut y = 40.0;
            for (rh, rw) in [(20.0, 200.0), (20.0, 90.0), (28.0, 200.0), (40.0, 200.0)] {
                let depth = bevel.min(rh * 0.2);
                let r = (rh * 0.5f32).min(8.0);
                pc.recess(Rect { x: x + 30.0, y, width: rw, height: rh }, (r, r, r, r), depth);
                y += rh + 20.0;
            }
            pc.boss(Rect { x: x + 30.0, y, width: 120.0, height: 30.0 }, (8.0, 8.0, 8.0, 8.0), 6.0);
        }
        pc.finish()
    }

    /// A carve grouped into its plate shades exactly as the same carve drawn
    /// as an overlay: the two plates are the same pixels. Until 2026-10-02 the
    /// grouped one drew a doubled outline — the shade line taken from the
    /// carves' slope fired twice down every wall — on every device, which
    /// was first taken for an NVIDIA quirk.
    #[test]
    fn a_grouped_carve_is_drawn_as_its_overlay_is() {
        // The style registry loads the config lazily; load it before the
        // tessellation, so both plates are shaded under one configuration.
        let _ = crate::layout::corner_shape();
        let (w, h, scale) = (540.0f32, 280.0f32, 2.0f32);
        let dl = two_plates(w, h);
        // Not vacuous: the left plate's five carves really are grouped.
        let (_, _, _, features) = crate::backend::tessellate::tessellate_display_list(&dl, w, h, scale);
        assert_eq!(features.len(), 5, "the left plate's carves should group");
        let Some(img) = render(&dl, w, h, scale) else {
            eprintln!("skipping: no Vulkan device");
            return;
        };
        let half = (w - 30.0) * 0.5;
        let shift = ((10.0 + half) * scale) as u32;
        let (x0, x1) = ((10.0 * scale) as u32, ((10.0 + half) * scale) as u32);
        let face = img.rgba((60.0 * scale) as u32, (25.0 * scale) as u32);
        let (mut differ, mut worst, mut carved) = (0usize, 0u8, 0usize);
        for y in 0..img.height {
            for x in x0..x1 {
                let (a, b) = (img.rgba(x, y), img.rgba(x + shift, y));
                let d = (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap();
                if d > 1 {
                    differ += 1;
                }
                worst = worst.max(d);
                if (0..3).any(|c| a[c].abs_diff(face[c]) > 8) {
                    carved += 1;
                }
            }
        }
        // The plates' own silhouettes and rolls are in the count too; what
        // matters is that the carves made marks at all.
        assert!(carved > 2000, "the carves left too little on the plate to compare ({carved} px)");
        assert_eq!(differ, 0, "grouped and overlay plates differ in {differ} px (worst channel {worst})");
    }
}
