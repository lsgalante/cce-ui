//! The stage's targets: the full-size backdrop image (also the blur-behind source) and the depth
//! buffer, sized to the surface, rebuilt on a resize and freed.

use super::*;

impl SceneStage {
    /// The size the backdrop and depth targets are made at for a surface of
    /// `surface`: the surface's, once a scene has been staged; 1×1 before.
    ///
    /// Most windows never draw a 3D scene, and full-surface targets cost
    /// ~33 MiB a window at 2560×1600 (D32 depth plus the RGBA backdrop). A
    /// 2D frame still samples the backdrop — the frosted root plate reads it,
    /// zeroed — and a zeroed 1×1 image reads the same under the UI's
    /// clamp-to-edge sampler at normalized coordinates. The first
    /// `stage_scene` / `stage_rt` grows them (`VkRenderer::want_scene_targets`).
    pub(crate) fn target_extent(&self, surface: vk::Extent2D) -> vk::Extent2D {
        if self.wanted {
            surface
        } else {
            vk::Extent2D { width: 1, height: 1 }
        }
    }

    pub(super) fn destroy_targets(&mut self, device: &ash::Device, allocator: &mut Allocator) {
        unsafe {
            if self.framebuffer != vk::Framebuffer::null() {
                device.destroy_framebuffer(self.framebuffer, None);
                self.framebuffer = vk::Framebuffer::null();
            }
            if self.backdrop_view != vk::ImageView::null() {
                device.destroy_image_view(self.backdrop_view, None);
                device.destroy_image(self.backdrop_image, None);
                self.backdrop_view = vk::ImageView::null();
                self.backdrop_image = vk::Image::null();
            }
            if self.depth_view != vk::ImageView::null() {
                device.destroy_image_view(self.depth_view, None);
                device.destroy_image(self.depth_image, None);
                self.depth_view = vk::ImageView::null();
                self.depth_image = vk::Image::null();
            }
        }
        if let Some(a) = self.backdrop_allocation.take() {
            let _ = allocator.free(a);
        }
        if let Some(a) = self.depth_allocation.take() {
            let _ = allocator.free(a);
        }
    }

    /// (Re)create the backdrop + depth targets at `extent`. Caller must have the
    /// device idle (the renderer's swapchain-rebuild path guarantees it) and must
    /// re-point the UI descriptor at the new `backdrop_view` and re-init its layout.
    ///
    /// Returns whether the targets were recreated. At an unchanged extent they
    /// are kept, CONTENTS included: the backdrop still holds the last staged
    /// scene and `backdrop_valid` still says so, and an app that stages only
    /// when its scene changes will not stage again to repair it.
    pub(crate) fn resize(
        &mut self,
        device: &ash::Device,
        allocator: &mut Allocator,
        extent: vk::Extent2D,
    ) -> bool {
        if extent == self.extent && self.framebuffer != vk::Framebuffer::null() {
            return false;
        }
        self.destroy_targets(device, allocator);
        self.extent = extent;
        self.backdrop_valid = false;
        unsafe {
            let backdrop_image = device
                .create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(self.format)
                        .extent(vk::Extent3D {
                            width: extent.width,
                            height: extent.height,
                            depth: 1,
                        })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(
                            vk::ImageUsageFlags::COLOR_ATTACHMENT
                                | vk::ImageUsageFlags::SAMPLED
                                | vk::ImageUsageFlags::TRANSFER_SRC
                                | vk::ImageUsageFlags::TRANSFER_DST,
                        )
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )
                .expect("Failed to create backdrop image");
            let requirements = device.get_image_memory_requirements(backdrop_image);
            let allocation = allocator
                .allocate(&AllocationCreateDesc {
                    name: "backdrop",
                    requirements,
                    location: MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .expect("Failed to allocate backdrop memory");
            device
                .bind_image_memory(backdrop_image, allocation.memory(), allocation.offset())
                .expect("Failed to bind backdrop memory");
            let backdrop_view = device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(backdrop_image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(self.format)
                        .subresource_range(
                            vk::ImageSubresourceRange::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .level_count(1)
                                .layer_count(1),
                        ),
                    None,
                )
                .expect("Failed to create backdrop view");
            self.backdrop_image = backdrop_image;
            self.backdrop_view = backdrop_view;
            self.backdrop_allocation = Some(allocation);

            let depth_image = device
                .create_image(
                    &vk::ImageCreateInfo::default()
                        .image_type(vk::ImageType::TYPE_2D)
                        .format(vk::Format::D32_SFLOAT)
                        .extent(vk::Extent3D {
                            width: extent.width,
                            height: extent.height,
                            depth: 1,
                        })
                        .mip_levels(1)
                        .array_layers(1)
                        .samples(vk::SampleCountFlags::TYPE_1)
                        .tiling(vk::ImageTiling::OPTIMAL)
                        .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
                        .initial_layout(vk::ImageLayout::UNDEFINED),
                    None,
                )
                .expect("Failed to create depth image");
            let requirements = device.get_image_memory_requirements(depth_image);
            let allocation = allocator
                .allocate(&AllocationCreateDesc {
                    name: "depth",
                    requirements,
                    location: MemoryLocation::GpuOnly,
                    linear: false,
                    allocation_scheme: AllocationScheme::GpuAllocatorManaged,
                })
                .expect("Failed to allocate depth memory");
            device
                .bind_image_memory(depth_image, allocation.memory(), allocation.offset())
                .expect("Failed to bind depth memory");
            let depth_view = device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(depth_image)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(vk::Format::D32_SFLOAT)
                        .subresource_range(
                            vk::ImageSubresourceRange::default()
                                .aspect_mask(vk::ImageAspectFlags::DEPTH)
                                .level_count(1)
                                .layer_count(1),
                        ),
                    None,
                )
                .expect("Failed to create depth view");
            self.depth_image = depth_image;
            self.depth_view = depth_view;
            self.depth_allocation = Some(allocation);

            let attachments = [self.backdrop_view, self.depth_view];
            self.framebuffer = device
                .create_framebuffer(
                    &vk::FramebufferCreateInfo::default()
                        .render_pass(self.render_pass)
                        .attachments(&attachments)
                        .width(extent.width)
                        .height(extent.height)
                        .layers(1),
                    None,
                )
                .expect("Failed to create scene framebuffer");
        }
        true
    }
}
