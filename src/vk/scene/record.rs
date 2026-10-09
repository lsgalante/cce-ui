//! A frame: its uniforms written into the frame's dynamic-offset buffer, and the pass recorded —
//! the mesh draws in staged order, each image and lit draw before the mesh draw it names.

use super::*;

impl SceneStage {
    /// After the frame fence: write this frame's per-draw uniforms (mvp + the
    /// window-corner info shader_3d shares with the 2D shader).
    pub(crate) fn write_frame_uniforms(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        frame_index: usize,
        corner_radius_px: f32,
    ) {
        let Some(staged) = &self.staged else {
            self.frames[frame_index].draw_count = 0;
            return;
        };
        let frame = &mut self.frames[frame_index];
        // One slot per mesh draw, then one per image, then one per lit draw.
        let slots = staged.draws.len() + staged.images.len() + staged.lit.len();
        let needed = self.uniform_stride * slots.max(1) as vk::DeviceSize;
        if needed > frame.uniforms.size {
            let mut old = std::mem::replace(&mut frame.uniforms, AllocatedBuffer::null());
            destroy_cpu_buffer(device, allocator, &mut old);
            frame.uniforms = create_cpu_buffer(
                device,
                allocator,
                needed.next_power_of_two(),
                vk::BufferUsageFlags::UNIFORM_BUFFER,
                "scene-uniforms",
            );
            Self::write_descriptor(device, frame);
        }
        let window_size = [self.extent.width as f32, self.extent.height as f32];
        let mapped = frame.uniforms.allocation.as_mut().unwrap().mapped_slice_mut().unwrap();
        let blocks = scene_uniforms(&staged.draws, &staged.images, window_size, corner_radius_px, self.light);
        for (i, uniforms) in blocks.iter().enumerate() {
            let offset = (self.uniform_stride as usize) * i;
            mapped[offset..offset + UNIFORM_SIZE as usize].copy_from_slice(bytemuck::bytes_of(uniforms));
        }
        let lit = lit_uniforms(&staged.lit, &self.lit_light, window_size, corner_radius_px);
        for (k, uniforms) in lit.iter().enumerate() {
            let offset = (self.uniform_stride as usize) * (blocks.len() + k);
            mapped[offset..offset + LIT_UNIFORM_SIZE].copy_from_slice(bytemuck::bytes_of(uniforms));
        }
        frame.draw_count = staged.draws.len() as u32;

        let verts = image_quads_3d(&staged.images);
        let bytes: &[u8] = bytemuck::cast_slice(&verts);
        if bytes.len() as vk::DeviceSize > frame.image_verts.size {
            let mut old = std::mem::replace(&mut frame.image_verts, AllocatedBuffer::null());
            destroy_cpu_buffer(device, allocator, &mut old);
            frame.image_verts = create_cpu_buffer(
                device,
                allocator,
                (bytes.len() as vk::DeviceSize).next_power_of_two(),
                vk::BufferUsageFlags::VERTEX_BUFFER,
                "scene-image-quads",
            );
        }
        if !bytes.is_empty() {
            frame.image_verts.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()
                [..bytes.len()]
                .copy_from_slice(bytes);
        }
    }

    /// Record the offscreen scene pass. Consumes the staged scene; afterwards the
    /// backdrop is in TRANSFER_SRC layout, ready for the swapchain copy. Returns
    /// false if nothing was staged.
    pub(crate) fn record(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame_index: usize,
        images: &crate::vk::image::ImageStage,
    ) -> bool {
        let Some(staged) = self.staged.take() else {
            return false;
        };
        let frame = &self.frames[frame_index];
        unsafe {
            let clear_values = [
                vk::ClearValue { color: vk::ClearColorValue { float32: [0.0, 0.0, 0.0, 0.0] } },
                vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 },
                },
            ];
            device.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(self.render_pass)
                    .framebuffer(self.framebuffer)
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: self.extent,
                    })
                    .clear_values(&clear_values),
                vk::SubpassContents::INLINE,
            );
            // Negative-height viewport: wgpu's Y-up NDC without touching winding.
            device.cmd_set_viewport(
                cmd,
                0,
                &[vk::Viewport {
                    x: 0.0,
                    y: self.extent.height as f32,
                    width: self.extent.width as f32,
                    height: -(self.extent.height as f32),
                    min_depth: 0.0,
                    max_depth: 1.0,
                }],
            );
            let (sx, sy, sw, sh) = staged.scissor;
            let sx = sx.min(self.extent.width);
            let sy = sy.min(self.extent.height);
            device.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D {
                    offset: vk::Offset2D { x: sx as i32, y: sy as i32 },
                    extent: vk::Extent2D {
                        width: sw.min(self.extent.width - sx),
                        height: sh.min(self.extent.height - sy),
                    },
                }],
            );
            let mut bound = self.pipeline;
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, bound);
            // The images due before mesh draw `at` (or, past the last draw,
            // every one left), in staged order.
            let draw_images = |at: usize, bound: &mut vk::Pipeline| {
                for (j, image) in staged.images.iter().enumerate() {
                    let due = (image.before as usize).min(staged.draws.len());
                    if due != at {
                        continue;
                    }
                    let Some(set) = images.descriptor_set(image.image) else { continue };
                    if *bound != self.image_pipeline {
                        device.cmd_bind_pipeline(
                            cmd,
                            vk::PipelineBindPoint::GRAPHICS,
                            self.image_pipeline,
                        );
                        *bound = self.image_pipeline;
                    }
                    device.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.image_pipeline_layout,
                        0,
                        &[frame.descriptor_set],
                        &[(self.uniform_stride as u32) * (staged.draws.len() + j) as u32],
                    );
                    device.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.image_pipeline_layout,
                        1,
                        &[set],
                        &[],
                    );
                    device.cmd_bind_vertex_buffers(cmd, 0, &[frame.image_verts.buffer], &[0]);
                    device.cmd_draw(cmd, 6, 1, (j * 6) as u32, 0);
                }
            };
            // The lit draws due before mesh draw `at`, likewise.
            let lit_base = staged.draws.len() + staged.images.len();
            let fallback = self.lit_fallback_image.and_then(|id| images.descriptor_set(id));
            let draw_lit = |at: usize, bound: &mut vk::Pipeline| {
                for (k, lit) in staged.lit.iter().enumerate() {
                    let due = (lit.before as usize).min(staged.draws.len());
                    if due != at {
                        continue;
                    }
                    let mesh = &self.lit_meshes[lit.mesh.0];
                    if mesh.count == 0 {
                        continue;
                    }
                    let set = lit.material.texture.and_then(|id| images.descriptor_set(id)).or(fallback);
                    let Some(set) = set else { continue };
                    if *bound != self.lit_pipeline {
                        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.lit_pipeline);
                        *bound = self.lit_pipeline;
                    }
                    if lit.wire_base_width > 0.0 {
                        let w = lit.wire_base_width.clamp(1.0, self.max_line_width);
                        let (constant, slope) = wire_base_bias(w);
                        device.cmd_set_depth_bias(cmd, constant, 0.0, slope);
                    } else {
                        device.cmd_set_depth_bias(cmd, 0.0, 0.0, 0.0);
                    }
                    device.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.image_pipeline_layout,
                        0,
                        &[frame.descriptor_set],
                        &[(self.uniform_stride as u32) * (lit_base + k) as u32],
                    );
                    device.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        self.image_pipeline_layout,
                        1,
                        &[set],
                        &[],
                    );
                    device.cmd_bind_vertex_buffers(cmd, 0, &[mesh.buffer.buffer], &[0]);
                    device.cmd_draw(cmd, mesh.count, 1, 0, 0);
                }
            };
            for (i, draw) in staged.draws.iter().enumerate() {
                draw_lit(i, &mut bound);
                draw_images(i, &mut bound);
                let mesh = &self.meshes[draw.mesh.0];
                if mesh.count == 0 {
                    continue;
                }
                // What it is drawn for: an instance mesh, or the one unit
                // instance that leaves it as it is.
                let (instance_buffer, instance_count) = match draw.instances {
                    Some(id) => {
                        let instances = &self.meshes[id.0];
                        (instances.buffer.buffer, instances.count)
                    }
                    None => (self.unit_instance.buffer, 1),
                };
                if instance_count == 0 {
                    continue;
                }
                let wanted = if draw.wireframe && draw.see_through {
                    self.wireframe_see_through_pipeline
                } else if draw.wireframe {
                    self.wireframe_pipeline.unwrap_or(self.pipeline)
                } else if draw.see_through {
                    self.see_through_pipeline
                } else {
                    self.pipeline
                };
                if wanted != bound {
                    device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, wanted);
                    bound = wanted;
                }
                if draw.wireframe && (draw.see_through || self.wireframe_pipeline.is_some()) {
                    device.cmd_set_line_width(cmd, draw.line_width.clamp(1.0, self.max_line_width));
                    device.cmd_set_depth_bias(cmd, 0.0, 0.0, 0.0);
                } else if draw.wire_base_width > 0.0 {
                    // Push this fill behind its coming wire overlay (see
                    // `wire_base_bias`).
                    let w = draw.wire_base_width.clamp(1.0, self.max_line_width);
                    let (constant, slope) = wire_base_bias(w);
                    device.cmd_set_depth_bias(cmd, constant, 0.0, slope);
                } else {
                    device.cmd_set_depth_bias(cmd, 0.0, 0.0, 0.0);
                }
                device.cmd_bind_descriptor_sets(
                    cmd,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.pipeline_layout,
                    0,
                    &[frame.descriptor_set],
                    &[(self.uniform_stride as u32) * i as u32],
                );
                device.cmd_bind_vertex_buffers(cmd, 0, &[mesh.buffer.buffer, instance_buffer], &[0, 0]);
                device.cmd_draw(cmd, mesh.count, instance_count, 0, 0);
            }
            draw_lit(staged.draws.len(), &mut bound);
            draw_images(staged.draws.len(), &mut bound);
            device.cmd_end_render_pass(cmd);
        }
        self.backdrop_valid = true;
        true
    }
}
