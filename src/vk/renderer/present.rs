//! Getting a frame to the screen: acquiring the swapchain image, the frame's damage and the
//! partial region it grows to, submitting, each image's age, and presenting with damage.

use super::*;

impl VkRenderer {
    /// Acquire the next swapchain image. None skips the frame: the swapchain is out of date
    /// (it is rebuilt next frame), the surface is lost, or the acquire failed.
    pub(super) unsafe fn acquire(&mut self, image_available: vk::Semaphore) -> Option<u32> {
        if present_debug() {
            eprintln!("[vk] frame {} acquire...", self.present_debug_count);
        }
        match self.swapchain_loader.acquire_next_image(
            self.swapchain,
            u64::MAX,
            image_available,
            vk::Fence::null(),
        ) {
            Ok((index, suboptimal)) => {
                if suboptimal {
                    self.swapchain_dirty = true;
                }
                Some(index)
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.swapchain_dirty = true;
                None
            }
            Err(result @ vk::Result::ERROR_SURFACE_LOST_KHR) => {
                self.mark_surface_lost(SurfaceLost { call: "vkAcquireNextImageKHR", result });
                None
            }
            Err(e) => {
                log::error!("acquire_next_image failed: {e:?}");
                None
            }
        }
    }

    /// This frame's damage, in physical px and inside the surface.
    pub(super) fn frame_damage(&self, frame2d: &Frame2D<'_>) -> Option<vk::Rect2D> {
        let full_scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: self.extent,
        };
        frame2d.damage.map(|(x, y, w, h)| {
            rect_intersect(
                vk::Rect2D {
                    offset: vk::Offset2D { x: x as i32, y: y as i32 },
                    extent: vk::Extent2D { width: w, height: h },
                },
                full_scissor,
            )
        })
    }

    /// The region of the acquired image this frame repaints, or None for all of it: the
    /// frame's damage, plus whatever the image missed of earlier frames. Backdrop (3D scene)
    /// frames copy whole images around and stay full. A frosted plate's blur reads the
    /// snapshot of the frame so far, and in a partial frame that snapshot is only right
    /// inside the region — outside it the image holds the previous FINAL frame, the plate and
    /// whatever covers it included. So a plate the region touches is repainted whole, with
    /// everything its blur can reach, and the grown region may touch the next plate.
    pub(super) fn partial_region(
        &self,
        frame2d: &Frame2D<'_>,
        damage: Option<vk::Rect2D>,
        image_index: u32,
        use_backdrop: bool,
        frost_exempt: Option<usize>,
    ) -> Option<vk::Rect2D> {
        let full_scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: self.extent,
        };
        let mut partial: Option<vk::Rect2D> = match (damage, self.image_ages[image_index as usize]) {
            _ if use_backdrop => None,
            (Some(d), ImageAge::Current) => Some(d),
            (Some(d), ImageAge::Behind(missing)) => Some(rect_union(d, missing)),
            _ => None,
        };
        if let Some(mut region) = partial {
            let frosts: Vec<vk::Rect2D> = frame2d
                .batches
                .iter()
                .enumerate()
                .filter(|&(i, b)| b.blur_behind && Some(i) != frost_exempt && b.start < b.end)
                .filter_map(|(_, b)| {
                    let verts = frame2d.verts.get(b.start as usize..b.end as usize)?;
                    verts_bounds(verts, self.extent)
                })
                .collect();
            loop {
                let mut grew = false;
                for &plate in &frosts {
                    let need = rect_intersect(rect_expand(plate, BLUR_REACH_PX), full_scissor);
                    if rect_overlaps(region, plate) && rect_intersect(region, need) != need {
                        region = rect_union(region, need);
                        grew = true;
                    }
                }
                if !grew {
                    break;
                }
            }
            partial = Some(region);
        }
        partial
    }

    /// Submit the frame. The acquire semaphore gates the swapchain image's first use: the
    /// backdrop copy (TRANSFER) or the UI pass (COLOR).
    pub(super) unsafe fn submit(&mut self, cmd: vk::CommandBuffer, image_available: vk::Semaphore, in_flight: vk::Fence, image_index: u32) {
        let wait_semaphores = [image_available];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
            | vk::PipelineStageFlags::TRANSFER];
        let cmds = [cmd];
        let signal_semaphores = [self.render_finished[image_index as usize]];
        let submit = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&cmds)
            .signal_semaphores(&signal_semaphores);
        self.core.device
            .queue_submit(self.core.queue, &[submit], in_flight)
            .expect("Queue submit failed");
        self.scene.submitted += 1;
    }

    /// The image now holds this frame; every other image fell behind by this frame's damage.
    pub(super) fn age_images(&mut self, image_index: u32, damage: Option<vk::Rect2D>) {
        for (k, age) in self.image_ages.iter_mut().enumerate() {
            *age = if k == image_index as usize {
                ImageAge::Current
            } else {
                match (damage, *age) {
                    (Some(d), ImageAge::Current) => ImageAge::Behind(d),
                    (Some(d), ImageAge::Behind(m)) => ImageAge::Behind(rect_union(d, m)),
                    _ => ImageAge::Unknown,
                }
            };
        }
    }

    /// Present the image. What changed since the previous present — the frame's damage,
    /// however much of the image had to be repainted to get there — rides as the present
    /// region, only for a frame that was itself partial: the first frames of a swapchain
    /// replace a buffer of another size or none at all.
    pub(super) unsafe fn present(&mut self, image_index: u32, damage: Option<vk::Rect2D>, partial: Option<vk::Rect2D>) {
        let full_scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: self.extent,
        };
        let signal_semaphores = [self.render_finished[image_index as usize]];
        let swapchains = [self.swapchain];
        let image_indices = [image_index];
        let present_rects = [vk::RectLayerKHR::default()
            .offset(damage.unwrap_or(full_scissor).offset)
            .extent(damage.unwrap_or(full_scissor).extent)
            .layer(0)];
        let present_region = [vk::PresentRegionKHR::default().rectangles(&present_rects)];
        let mut present_regions = vk::PresentRegionsKHR::default().regions(&present_region);
        let mut present = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);
        if self.core.incremental_present && partial.is_some() {
            present = present.push_next(&mut present_regions);
        }
        if present_debug() {
            eprintln!("[vk] frame {} present img {}...", self.present_debug_count, image_index);
        }
        match self.swapchain_loader.queue_present(self.core.queue, &present) {
            Ok(suboptimal) => {
                if suboptimal {
                    self.swapchain_dirty = true;
                }
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.swapchain_dirty = true;
            }
            Err(result @ vk::Result::ERROR_SURFACE_LOST_KHR) => {
                self.mark_surface_lost(SurfaceLost { call: "vkQueuePresentKHR", result });
            }
            Err(e) => log::error!("queue_present failed: {e:?}"),
        }
        if present_debug() {
            eprintln!("[vk] frame {} presented", self.present_debug_count);
            self.present_debug_count += 1;
        }
    }
}
