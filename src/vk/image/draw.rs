//! Drawing images: the frame's quad vertices, an image's descriptor set and view, and recording one
//! quad.

use super::*;

impl ImageStage {
    /// After the frame fence: build this frame's quad vertices (6 per image,
    /// in `images` order).
    pub(crate) fn write_frame_buffer(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        frame_index: usize,
        images: &[ImageQuad],
        extent: vk::Extent2D,
    ) {
        let verts = crate::draw::glyphs::image_quad_vertices(images, extent.width, extent.height);
        let bytes: &[u8] = bytemuck::cast_slice(&verts);
        let buf = &mut self.frame_buffers[frame_index];
        if bytes.len() as vk::DeviceSize > buf.size {
            let mut old = std::mem::replace(buf, AllocatedBuffer::null());
            destroy_cpu_buffer(device, allocator, &mut old);
            *buf = create_cpu_buffer(
                device,
                allocator,
                (bytes.len() as vk::DeviceSize).next_power_of_two(),
                vk::BufferUsageFlags::VERTEX_BUFFER,
                "image-quads",
            );
        }
        if !bytes.is_empty() {
            buf.allocation.as_mut().unwrap().mapped_slice_mut().unwrap()[..bytes.len()]
                .copy_from_slice(bytes);
        }
    }

    /// The descriptor set an uploaded image is drawn with, or None while its
    /// upload has not landed (or after it was freed).
    pub(crate) fn descriptor_set(&self, image_id: u32) -> Option<vk::DescriptorSet> {
        self.images.get(&image_id).map(|gpu| gpu.descriptor_set)
    }

    /// An uploaded image's view and size, for a pass that samples it under
    /// bindings of its own (the path tracer). None while its upload has not
    /// landed.
    pub(crate) fn view_and_size(&self, image_id: u32) -> Option<(vk::ImageView, u32, u32)> {
        self.images.get(&image_id).map(|gpu| (gpu.view, gpu.width, gpu.height))
    }

    /// Record one image quad (index `i` of this frame's list). The caller
    /// restores its own pipeline/scissor state afterwards. Returns false if the
    /// image hasn't finished uploading (draw skipped).
    pub(crate) fn record_quad(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame_index: usize,
        i: usize,
        image_id: u32,
    ) -> bool {
        let Some(gpu) = self.images.get(&image_id) else {
            return false;
        };
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[gpu.descriptor_set],
                &[],
            );
            device.cmd_bind_vertex_buffers(cmd, 0, &[self.frame_buffers[frame_index].buffer], &[0]);
            device.cmd_draw(cmd, 6, 1, (i * 6) as u32, 0);
        }
        true
    }
}
