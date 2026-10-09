//! The surface and its swapchain: creation and re-creation, surface loss, moving the renderer
//! between surfaces, resizing, and the window-info block the shaders read.

use super::*;

impl VkRenderer {

    /// The window-clip corner radius as the shaders consume it: the nominal
    /// radius widened by the curvature-match factor, so the clip cuts along
    /// the same curve as window-scale plate corners (`plate_push_raised` with
    /// `scale_corners`) and a clipped window reads the same as a plate-drawn
    /// one. Capped at half the smaller extent, like the plate path's cap.
    pub(super) fn clip_corner_radius(&self) -> f32 {
        let cap = 0.5 * self.extent.width.min(self.extent.height) as f32;
        (self.corner_radius_px * crate::layout::corner_span_factor()).min(cap)
    }

    /// The pinned relief heights in physical px, 0 = follow the width.
    pub(super) fn relief_px(&self) -> (f32, f32) {
        relief_px_at(crate::scale::scale_factor())
    }

    pub(super) fn write_window_info(&mut self) {
        // [size/clip vec4][carve profile meta vec4][8 vec4 carve slopes]
        // [roll profile meta vec4][8 vec4 roll slopes][relief heights vec4]
        // — must stay in lockstep with shader2d's WindowInfo. (The frost
        // recipe is per plate, in its push block, since RFC material step 3.)
        let relief = self.relief_px();
        let data = window_info_data(self.extent, self.clip_corner_radius(), relief);
        self.relief_uploaded = relief;
        // Every pixel shades differently now: no image may be patched.
        self.image_ages.fill(ImageAge::Unknown);
        self.profile_gen = crate::layout::bevel_profile_generation();
        self.roll_profile_gen = crate::layout::roll_profile_generation();
        if let Some(allocation) = self.window_info.allocation.as_mut() {
            allocation.mapped_slice_mut().unwrap()[..WINDOW_INFO_BYTES as usize]
                .copy_from_slice(bytemuck::cast_slice(&data));
        }
    }

    pub(super) fn destroy_swapchain_resources(&mut self) {
        unsafe {
            for fb in self.framebuffers.drain(..) {
                self.core.device.destroy_framebuffer(fb, None);
            }
            for view in self.swapchain_views.drain(..) {
                self.core.device.destroy_image_view(view, None);
            }
            self.swapchain_images.clear();
            for sem in self.render_finished.drain(..) {
                self.core.device.destroy_semaphore(sem, None);
            }
        }
    }

    /// Build the swapchain for the current surface. Only the calls that ask
    /// the surface can fail with [`SurfaceLost`]; everything after them is
    /// device work and still panics as the bug it would be. A failure leaves
    /// the previous swapchain (if any) in `self.swapchain` for Drop.
    pub(super) fn create_swapchain(&mut self) -> Result<(), SurfaceLost> {
        unsafe {
            let caps = self.core
                .surface_loader
                .get_physical_device_surface_capabilities(self.core.physical_device, self.surface)
                .map_err(|result| SurfaceLost {
                    call: "vkGetPhysicalDeviceSurfaceCapabilitiesKHR",
                    result,
                })?;

            // Wayland reports "extent defined by the swapchain" (u32::MAX); use the
            // size the configure events gave us.
            let extent = if caps.current_extent.width != u32::MAX {
                caps.current_extent
            } else {
                vk::Extent2D {
                    width: self
                        .desired_extent
                        .width
                        .clamp(caps.min_image_extent.width, caps.max_image_extent.width.max(1)),
                    height: self
                        .desired_extent
                        .height
                        .clamp(caps.min_image_extent.height, caps.max_image_extent.height.max(1)),
                }
            };

            // One more than the minimum, so acquiring never waits on the
            // compositor to release one — except where the caller asked for
            // the minimum (`set_minimal_swapchain`).
            let mut image_count = caps.min_image_count + u32::from(!self.minimal_swapchain);
            if caps.max_image_count > 0 {
                image_count = image_count.min(caps.max_image_count);
            }

            // Prefer premultiplied (what the DE's other clients pick), else opaque,
            // else whatever the surface offers.
            let composite_alpha = [
                vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
                vk::CompositeAlphaFlagsKHR::OPAQUE,
                vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
                vk::CompositeAlphaFlagsKHR::INHERIT,
            ]
            .into_iter()
            .find(|&mode| caps.supported_composite_alpha.contains(mode))
            .unwrap_or(vk::CompositeAlphaFlagsKHR::OPAQUE);

            // MAILBOX when the driver offers it (Mesa Wayland always does):
            // FIFO's present throttle waits on the PREVIOUS present's frame
            // callback, and a surface the compositor never renders (off the
            // viewport) never gets one — the second-ever present then blocks
            // forever inside queue_present with the whole event loop behind
            // it. MAILBOX just replaces the queued buffer, so presenting to
            // an invisible surface is always safe. The demand-driven loop's
            // frame-callback gate keeps MAILBOX from free-running.
            let modes = self
                .core
                .surface_loader
                .get_physical_device_surface_present_modes(self.core.physical_device, self.surface)
                .unwrap_or_default();
            self.present_mode = if modes.contains(&vk::PresentModeKHR::MAILBOX) {
                vk::PresentModeKHR::MAILBOX
            } else {
                vk::PresentModeKHR::FIFO
            };

            let old_swapchain = self.swapchain;
            self.swapchain = self
                .swapchain_loader
                .create_swapchain(
                    &vk::SwapchainCreateInfoKHR::default()
                        .surface(self.surface)
                        .min_image_count(image_count)
                        .image_format(self.surface_format.format)
                        .image_color_space(self.surface_format.color_space)
                        .image_extent(extent)
                        .image_array_layers(1)
                        .image_usage(
                            vk::ImageUsageFlags::COLOR_ATTACHMENT
                                | vk::ImageUsageFlags::TRANSFER_DST
                                // Blur-behind plates copy the frame-so-far out
                                // of the swapchain into the snapshot image.
                                | vk::ImageUsageFlags::TRANSFER_SRC,
                        )
                        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                        .pre_transform(caps.current_transform)
                        .composite_alpha(composite_alpha)
                        .present_mode(self.present_mode)
                        .clipped(true)
                        .old_swapchain(old_swapchain),
                    None,
                )
                .map_err(|result| SurfaceLost { call: "vkCreateSwapchainKHR", result })?;
            if old_swapchain != vk::SwapchainKHR::null() {
                self.swapchain_loader.destroy_swapchain(old_swapchain, None);
            }
            self.extent = extent;
            if extent.width == self.desired_extent.width && extent.height == self.desired_extent.height {
                // Keep the two in step so a rebuild queued for a non-resize
                // reason (suboptimal/out-of-date) doesn't hand
                // `pending_extent` a stale or unclamped size.
                self.desired_extent = extent;
            } else {
                // The surface capabilities overrode the requested size (seen
                // on suspend/resume, when caps briefly lag the real surface
                // state). Presenting this swapchain would commit a buffer the
                // caller never approved — paired with the wrong buffer scale
                // that reads as a self-resize and half/double-sizes the
                // window. Keep the request, requeue the rebuild, and let
                // draw_frame skip the present until caps agree.
                log::warn!(
                    "swapchain extent {}x{} != requested {}x{}; skipping present until they agree",
                    extent.width, extent.height,
                    self.desired_extent.width, self.desired_extent.height,
                );
                self.swapchain_dirty = true;
            }

            let images = self
                .swapchain_loader
                .get_swapchain_images(self.swapchain)
                .map_err(|result| SurfaceLost { call: "vkGetSwapchainImagesKHR", result })?;
            self.swapchain_images = images.clone();
            self.image_ages = vec![ImageAge::Unknown; images.len()];
            let subresource_range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);
            for image in &images {
                let view = self.core
                    .device
                    .create_image_view(
                        &vk::ImageViewCreateInfo::default()
                            .image(*image)
                            .view_type(vk::ImageViewType::TYPE_2D)
                            .format(self.surface_format.format)
                            .subresource_range(subresource_range),
                        None,
                    )
                    .expect("Failed to create swapchain view");
                self.swapchain_views.push(view);
                let attachments = [view];
                let fb = self.core
                    .device
                    .create_framebuffer(
                        &vk::FramebufferCreateInfo::default()
                            .render_pass(self.render_pass)
                            .attachments(&attachments)
                            .width(extent.width)
                            .height(extent.height)
                            .layers(1),
                        None,
                    )
                    .expect("Failed to create framebuffer");
                self.framebuffers.push(fb);
                self.render_finished.push(
                    self.core.device
                        .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                        .unwrap(),
                );
            }
        }
        Ok(())
    }

    pub(super) fn recreate_swapchain(&mut self) -> Result<(), SurfaceLost> {
        unsafe {
            let _ = self.core.device.device_wait_idle();
        }
        let before = self.extent;
        self.destroy_swapchain_resources();
        self.create_swapchain()?;
        if present_debug() {
            eprintln!(
                "[vk] swapchain rebuilt {}x{} -> {}x{} (backdrop valid: {})",
                before.width, before.height, self.extent.width, self.extent.height,
                self.scene.backdrop_valid,
            );
        }
        self.write_window_info();
        self.sync_backdrop_targets();
        Ok(())
    }

    /// Latch a lost surface: say so once, then skip draws until a new
    /// surface is attached.
    pub(super) fn mark_surface_lost(&mut self, lost: SurfaceLost) {
        if !self.surface_lost {
            log::warn!("[vk] {lost}; skipping draws until the connection is replaced");
        }
        self.surface_lost = true;
    }

    /// Whether the surface has been reported lost (see [`SurfaceLost`]). A
    /// caller with its own event loop can end its session on this rather
    /// than wait for the connection error.
    pub fn surface_lost(&self) -> bool {
        self.surface_lost
    }

    /// Use the surface's minimum swapchain image count instead of one more.
    /// For a surface that redraws rarely and is large — the desktop grid's
    /// patch is ~240 MiB an image on a HiDPI panel — where the spare image
    /// costs more than an occasional wait on the compositor. Takes effect at
    /// the next swapchain rebuild.
    pub fn set_minimal_swapchain(&mut self) {
        if !self.minimal_swapchain {
            self.minimal_swapchain = true;
            self.swapchain_dirty = true;
        }
    }

    /// Let go of the window surface: wait idle, then destroy the swapchain
    /// and the `VkSurfaceKHR`, keeping the device, pipelines and atlases. The
    /// `wl_surface` under them may be destroyed after this returns, and must
    /// not be before — a swapchain presenting to a dead surface is undefined.
    /// Until [`attach_surface`](Self::attach_surface), `draw_frame_2d` draws
    /// nothing and returns false.
    pub fn detach_surface(&mut self) {
        unsafe {
            let _ = self.core.device.device_wait_idle();
            self.destroy_swapchain_resources();
            if self.swapchain != vk::SwapchainKHR::null() {
                self.swapchain_loader.destroy_swapchain(self.swapchain, None);
                self.swapchain = vk::SwapchainKHR::null();
            }
            if self.surface != vk::SurfaceKHR::null() {
                self.core.surface_loader.destroy_surface(self.surface, None);
                self.surface = vk::SurfaceKHR::null();
            }
        }
        self.extent = vk::Extent2D { width: 0, height: 0 };
        self.swapchain_dirty = true;
    }

    /// Present to a different `wl_surface` from now on, at `width` x
    /// `height` physical px — detaching from the current one first if it is
    /// still attached. What makes a popup surface cheap to re-open: a new
    /// renderer costs a device and every pipeline, this costs one swapchain.
    ///
    /// # Safety
    /// Same contract as [`VkRenderer::try_new`]: live `wl_display` / `wl_surface`
    /// pointers that outlive the attachment.
    pub unsafe fn attach_surface(
        &mut self,
        display_ptr: *mut c_void,
        surface_ptr: *mut c_void,
        width: u32,
        height: u32,
    ) -> Result<(), SurfaceLost> {
        self.attach_surface_to(
            crate::vk::core::SurfaceTarget::Wayland { display: display_ptr, surface: surface_ptr },
            width,
            height,
        )
    }

    /// [`attach_surface`](Self::attach_surface) for any window
    /// [`SurfaceTarget`](crate::vk::core::SurfaceTarget).
    ///
    /// # Safety
    /// The target's pointers must be live and outlive the attachment.
    pub unsafe fn attach_surface_to(
        &mut self,
        target: crate::vk::core::SurfaceTarget,
        width: u32,
        height: u32,
    ) -> Result<(), SurfaceLost> {
        if self.surface != vk::SurfaceKHR::null() {
            self.detach_surface();
        }
        self.surface = self.core.create_surface(target)?;
        self.surface_lost = false;
        self.resize(width, height);
        self.swapchain_dirty = true;
        Ok(())
    }

    /// Whether a surface is attached — false between
    /// [`detach_surface`](Self::detach_surface) and the next attach.
    pub fn has_surface(&self) -> bool {
        self.surface != vk::SurfaceKHR::null()
    }

    /// The extent the next `draw_frame` will render at: the pending size when a
    /// swapchain rebuild is queued, otherwise the live one.
    pub fn pending_extent(&self) -> vk::Extent2D {
        if self.swapchain_dirty { self.desired_extent } else { self.extent }
    }

    /// Request a new physical size (from xdg configure / scale changes). Applied
    /// lazily on the next `draw_frame`.
    pub fn resize(&mut self, width: u32, height: u32) {
        let extent = vk::Extent2D { width: width.max(1), height: height.max(1) };
        // Record the request unconditionally, not just when it differs from the
        // live extent: with a rebuild already queued (`swapchain_dirty`), a
        // request that returns to the live size must overwrite the queued one.
        // Otherwise `desired_extent` stays wedged at the intermediate size, the
        // caller's `pending_extent` gate never matches, and no frame presents
        // again — a resume scale bounce (2→1→2 before any draw) froze the
        // status-bar clock exactly this way.
        self.desired_extent = extent;
        if extent.width != self.extent.width || extent.height != self.extent.height {
            self.swapchain_dirty = true;
        }
    }

    // Used at cutover, when scale changes re-derive the radius; vk-smoke fixes it at init.
    // Nominal (circle-equivalent) radius in physical px — the curvature-match
    // widening for squircle corner shapes happens at consumption
    // (`clip_corner_radius`), so callers pass the configured radius as-is.
    #[allow(dead_code)]
    pub fn set_corner_radius(&mut self, radius_px: f32) {
        self.corner_radius_px = radius_px;
        // Written on the next swapchain rebuild or draw-idle moment; a mapped write
        // here would race in-flight frames, so route it through the dirty path.
        self.swapchain_dirty = true;
    }
}
